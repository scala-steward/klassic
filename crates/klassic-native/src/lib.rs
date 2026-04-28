use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;

use klassic_rewrite::rewrite_expression;
use klassic_span::{Diagnostic, SourceFile, Span};
use klassic_syntax::{
    BinaryOp, Expr, FloatLiteralKind, TypeAnnotation, TypeClassConstraint, UnaryOp,
    parse_inline_expression, parse_source,
};
use klassic_types::typecheck_program;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeCompilerConfig {
    pub deny_trust: bool,
    pub warn_trust: bool,
}

#[derive(Clone, Debug)]
pub struct NativeCompileError {
    source: SourceFile,
    diagnostic: Diagnostic,
}

impl NativeCompileError {
    fn new(source: SourceFile, diagnostic: Diagnostic) -> Self {
        Self { source, diagnostic }
    }

    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for NativeCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.diagnostic.render(&self.source))
    }
}

impl std::error::Error for NativeCompileError {}

#[allow(clippy::result_large_err)]
pub fn compile_source_to_elf(
    name: &str,
    text: &str,
    config: NativeCompilerConfig,
) -> Result<Vec<u8>, NativeCompileError> {
    let source = SourceFile::new(name, text);
    let expr = parse_source(&source)
        .map_err(|diagnostic| NativeCompileError::new(source.clone(), diagnostic))?;
    let expr = rewrite_expression(expr);
    typecheck_program(&expr)
        .map_err(|diagnostic| NativeCompileError::new(source.clone(), diagnostic))?;
    analyze_proofs(&expr, config)
        .map_err(|diagnostic| NativeCompileError::new(source.clone(), diagnostic))?;
    let object = NativeCodeGenerator::new(source.clone())
        .compile(&expr)
        .map_err(|diagnostic| NativeCompileError::new(source, diagnostic))?;
    Ok(elf::write_executable(object))
}

#[derive(Clone, Debug)]
struct ProofDefinition {
    name: String,
    span: Span,
    proposition: Expr,
    body: Option<Expr>,
    trusted: bool,
    is_axiom: bool,
}

#[derive(Clone, Debug)]
struct ProofMetadata {
    name: String,
    span: Span,
    level: usize,
    trusted: bool,
    dependencies: HashSet<String>,
}

fn analyze_proofs(expr: &Expr, config: NativeCompilerConfig) -> Result<(), Diagnostic> {
    let definitions = collect_proof_definitions(expr);
    if definitions.is_empty() {
        return Ok(());
    }
    let metadata = compute_proof_metadata(&definitions)?;
    if config.deny_trust
        && let Some(proof) = definitions
            .iter()
            .filter_map(|definition| metadata.get(&definition.name))
            .find(|proof| proof.trusted)
    {
        return Err(Diagnostic::compile(
            proof.span,
            format!(
                "trusted proof '{}' is not allowed (level {})",
                proof.name, proof.level
            ),
        ));
    }
    if config.warn_trust {
        let mut proofs = metadata
            .values()
            .filter(|proof| proof.trusted)
            .cloned()
            .collect::<Vec<_>>();
        proofs.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
        for proof in proofs {
            let mut deps = proof.dependencies.iter().cloned().collect::<Vec<_>>();
            deps.sort();
            eprintln!(
                "[trust] proof '{}' is trusted (level {}); depends on [{}]",
                proof.name,
                proof.level,
                deps.join(", ")
            );
        }
    }
    Ok(())
}

fn collect_proof_definitions(expr: &Expr) -> Vec<ProofDefinition> {
    let expressions = match expr {
        Expr::Block { expressions, .. } => expressions.as_slice(),
        other => std::slice::from_ref(other),
    };
    expressions
        .iter()
        .filter_map(|expression| match expression {
            Expr::TheoremDeclaration {
                name,
                proposition,
                body,
                trusted,
                span,
                ..
            } => Some(ProofDefinition {
                name: name.clone(),
                span: *span,
                proposition: proposition.as_ref().clone(),
                body: Some(body.as_ref().clone()),
                trusted: *trusted,
                is_axiom: false,
            }),
            Expr::AxiomDeclaration {
                name,
                proposition,
                span,
                ..
            } => Some(ProofDefinition {
                name: name.clone(),
                span: *span,
                proposition: proposition.as_ref().clone(),
                body: None,
                trusted: true,
                is_axiom: true,
            }),
            _ => None,
        })
        .collect()
}

fn compute_proof_metadata(
    definitions: &[ProofDefinition],
) -> Result<HashMap<String, ProofMetadata>, Diagnostic> {
    let by_name = definitions
        .iter()
        .map(|definition| (definition.name.clone(), definition))
        .collect::<HashMap<_, _>>();
    let proof_names = by_name.keys().cloned().collect::<HashSet<_>>();
    let deps = definitions
        .iter()
        .map(|definition| {
            (
                definition.name.clone(),
                proof_dependencies(definition, &proof_names),
            )
        })
        .collect::<HashMap<_, _>>();

    fn compute_level(
        name: &str,
        by_name: &HashMap<String, &ProofDefinition>,
        deps: &HashMap<String, HashSet<String>>,
        memo: &mut HashMap<String, usize>,
        visiting: &mut HashSet<String>,
    ) -> Result<usize, Diagnostic> {
        if let Some(level) = memo.get(name) {
            return Ok(*level);
        }
        if !visiting.insert(name.to_string()) {
            let span = by_name
                .get(name)
                .map(|definition| definition.span)
                .unwrap_or(Span::new(0, 0));
            return Err(Diagnostic::compile(
                span,
                format!("cyclic proof dependency detected for '{name}'"),
            ));
        }
        let definition = by_name
            .get(name)
            .expect("proof definition should exist for level computation");
        let base = if definition.is_axiom || definition.trusted {
            1
        } else {
            0
        };
        let mut level = base;
        if let Some(children) = deps.get(name) {
            for dependency in children {
                let dep_level = compute_level(dependency, by_name, deps, memo, visiting)?;
                if dep_level > 0 {
                    level = level.max(dep_level + 1);
                }
            }
        }
        visiting.remove(name);
        memo.insert(name.to_string(), level);
        Ok(level)
    }

    let mut memo = HashMap::new();
    let mut visiting = HashSet::new();
    for definition in definitions {
        compute_level(&definition.name, &by_name, &deps, &mut memo, &mut visiting)?;
    }

    Ok(definitions
        .iter()
        .map(|definition| {
            let level = *memo
                .get(&definition.name)
                .expect("proof level should exist after computation");
            (
                definition.name.clone(),
                ProofMetadata {
                    name: definition.name.clone(),
                    span: definition.span,
                    level,
                    trusted: level > 0,
                    dependencies: deps.get(&definition.name).cloned().unwrap_or_default(),
                },
            )
        })
        .collect())
}

fn proof_dependencies(definition: &ProofDefinition, names: &HashSet<String>) -> HashSet<String> {
    let mut dependencies = HashSet::new();
    collect_referenced_proof_names(&definition.proposition, names, &mut dependencies);
    if let Some(body) = &definition.body {
        collect_referenced_proof_names(body, names, &mut dependencies);
    }
    dependencies.remove(&definition.name);
    dependencies
}

fn collect_referenced_proof_names(
    expr: &Expr,
    names: &HashSet<String>,
    dependencies: &mut HashSet<String>,
) {
    match expr {
        Expr::Identifier { name, .. } if names.contains(name) => {
            dependencies.insert(name.clone());
        }
        Expr::TheoremDeclaration {
            proposition, body, ..
        } => {
            collect_referenced_proof_names(proposition, names, dependencies);
            collect_referenced_proof_names(body, names, dependencies);
        }
        Expr::AxiomDeclaration { proposition, .. } => {
            collect_referenced_proof_names(proposition, names, dependencies);
        }
        Expr::VarDecl { value, .. }
        | Expr::Assign { value, .. }
        | Expr::Unary { expr: value, .. } => {
            collect_referenced_proof_names(value, names, dependencies);
        }
        Expr::DefDecl { body, .. } | Expr::Lambda { body, .. } => {
            collect_referenced_proof_names(body, names, dependencies);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_referenced_proof_names(lhs, names, dependencies);
            collect_referenced_proof_names(rhs, names, dependencies);
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            collect_referenced_proof_names(callee, names, dependencies);
            for argument in arguments {
                collect_referenced_proof_names(argument, names, dependencies);
            }
        }
        Expr::FieldAccess { target, .. } => {
            collect_referenced_proof_names(target, names, dependencies);
        }
        Expr::Cleanup { body, cleanup, .. } => {
            collect_referenced_proof_names(body, names, dependencies);
            collect_referenced_proof_names(cleanup, names, dependencies);
        }
        Expr::RecordConstructor { arguments, .. }
        | Expr::ListLiteral {
            elements: arguments,
            ..
        }
        | Expr::SetLiteral {
            elements: arguments,
            ..
        } => {
            for argument in arguments {
                collect_referenced_proof_names(argument, names, dependencies);
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_referenced_proof_names(value, names, dependencies);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (key, value) in entries {
                collect_referenced_proof_names(key, names, dependencies);
                collect_referenced_proof_names(value, names, dependencies);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_referenced_proof_names(condition, names, dependencies);
            collect_referenced_proof_names(then_branch, names, dependencies);
            if let Some(branch) = else_branch {
                collect_referenced_proof_names(branch, names, dependencies);
            }
        }
        Expr::While {
            condition, body, ..
        } => {
            collect_referenced_proof_names(condition, names, dependencies);
            collect_referenced_proof_names(body, names, dependencies);
        }
        Expr::Foreach { iterable, body, .. } => {
            collect_referenced_proof_names(iterable, names, dependencies);
            collect_referenced_proof_names(body, names, dependencies);
        }
        Expr::Block { expressions, .. } => {
            for expression in expressions {
                collect_referenced_proof_names(expression, names, dependencies);
            }
        }
        Expr::InstanceDeclaration { methods, .. } => {
            for method in methods {
                collect_referenced_proof_names(method, names, dependencies);
            }
        }
        Expr::Int { .. }
        | Expr::Double { .. }
        | Expr::Bool { .. }
        | Expr::String { .. }
        | Expr::Null { .. }
        | Expr::Unit { .. }
        | Expr::Identifier { .. }
        | Expr::ModuleHeader { .. }
        | Expr::Import { .. }
        | Expr::RecordDeclaration { .. }
        | Expr::TypeClassDeclaration { .. }
        | Expr::PegRuleBlock { .. } => {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeValue {
    Int,
    Bool,
    Null,
    Unit,
    StaticFloat { bits: u32 },
    StaticDouble { bits: u64 },
    StaticString { label: DataLabel, len: usize },
    RuntimeString { data: DataLabel, len: DataLabel },
    RuntimeLinesList { data: DataLabel, len: DataLabel },
    StaticIntList { label: DataLabel, len: usize },
    StaticList { label: ListLabel },
    StaticRecord { label: RecordLabel },
    StaticMap { label: MapLabel },
    StaticSet { label: SetLabel },
    StaticLambda { label: LambdaLabel },
    BuiltinFunction { label: BuiltinLabel },
}

#[derive(Clone, Copy, Debug)]
struct NativeStringRef {
    data: DataLabel,
    len: NativeStringLen,
}

#[derive(Clone, Copy, Debug)]
enum NativeStringLen {
    Immediate(usize),
    Runtime(DataLabel),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StaticValue {
    Int(i64),
    Float(u32),
    Double(u64),
    Bool(bool),
    Null,
    Unit,
    StaticString { label: DataLabel, len: usize },
    StaticIntList { label: DataLabel, len: usize },
    StaticList { label: ListLabel },
    StaticRecord { label: RecordLabel },
    StaticMap { label: MapLabel },
    StaticSet { label: SetLabel },
    StaticLambda { label: LambdaLabel },
    BuiltinFunction { label: BuiltinLabel },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticRecord {
    name: String,
    fields: Vec<(String, StaticValue)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticList {
    elements: Vec<StaticValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticMap {
    entries: Vec<(StaticValue, StaticValue)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticSet {
    elements: Vec<StaticValue>,
}

#[derive(Clone, Debug)]
struct StaticLambda {
    params: Vec<String>,
    body: Expr,
    captures: HashMap<String, StaticValue>,
    runtime_captures: HashMap<String, VarSlot>,
    contains_thread_call: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct QueuedThread {
    body: Expr,
    captures: HashMap<String, StaticValue>,
    runtime_captures: HashMap<String, VarSlot>,
}

struct NativeCodeGenerator {
    source: SourceFile,
    asm: Assembler,
    newline: DataLabel,
    true_text: DataLabel,
    false_text: DataLabel,
    null_text: DataLabel,
    unit_text: DataLabel,
    list_open: DataLabel,
    list_close: DataLabel,
    map_open: DataLabel,
    set_open: DataLabel,
    hash: DataLabel,
    paren_open: DataLabel,
    paren_close: DataLabel,
    comma_space: DataLabel,
    colon_space: DataLabel,
    print_i64: TextLabel,
    command_line_argc: DataLabel,
    command_line_argv1_base: DataLabel,
    environment_base: DataLabel,
    scopes: Vec<HashMap<String, VarSlot>>,
    static_scopes: Vec<HashMap<String, StaticValue>>,
    scope_base_offsets: Vec<i32>,
    next_stack_offset: i32,
    functions: HashMap<String, NativeFunction>,
    function_order: Vec<String>,
    referenced_functions: HashSet<String>,
    instance_methods: Vec<NativeInstanceMethod>,
    record_schemas: HashMap<String, Vec<String>>,
    static_lists: Vec<StaticList>,
    static_records: Vec<StaticRecord>,
    static_maps: Vec<StaticMap>,
    static_sets: Vec<StaticSet>,
    static_lambdas: Vec<StaticLambda>,
    builtin_aliases: Vec<String>,
    module_aliases: HashMap<String, String>,
    virtual_files: HashMap<String, String>,
    virtual_dirs: HashSet<String>,
    unknown_virtual_paths: HashSet<String>,
    queued_threads: Vec<QueuedThread>,
    dynamic_control_depth: usize,
    mergeable_dynamic_branch_depth: usize,
}

impl NativeCodeGenerator {
    fn new(source: SourceFile) -> Self {
        let mut asm = Assembler::new();
        let newline = asm.data_label_with_bytes(b"\n");
        let true_text = asm.data_label_with_bytes(b"true");
        let false_text = asm.data_label_with_bytes(b"false");
        let null_text = asm.data_label_with_bytes(b"null");
        let unit_text = asm.data_label_with_bytes(b"()");
        let list_open = asm.data_label_with_bytes(b"[");
        let list_close = asm.data_label_with_bytes(b"]");
        let map_open = asm.data_label_with_bytes(b"%[");
        let set_open = asm.data_label_with_bytes(b"%(");
        let hash = asm.data_label_with_bytes(b"#");
        let paren_open = asm.data_label_with_bytes(b"(");
        let paren_close = asm.data_label_with_bytes(b")");
        let comma_space = asm.data_label_with_bytes(b", ");
        let colon_space = asm.data_label_with_bytes(b": ");
        let print_i64 = asm.create_text_label();
        let command_line_argc = asm.data_label_with_i64s(&[0]);
        let command_line_argv1_base = asm.data_label_with_i64s(&[0]);
        let environment_base = asm.data_label_with_i64s(&[0]);
        let mut record_schemas = HashMap::new();
        record_schemas.insert("Point".to_string(), vec!["x".to_string(), "y".to_string()]);
        Self {
            source,
            asm,
            newline,
            true_text,
            false_text,
            null_text,
            unit_text,
            list_open,
            list_close,
            map_open,
            set_open,
            hash,
            paren_open,
            paren_close,
            comma_space,
            colon_space,
            print_i64,
            command_line_argc,
            command_line_argv1_base,
            environment_base,
            scopes: vec![HashMap::new()],
            static_scopes: vec![HashMap::new()],
            scope_base_offsets: vec![0],
            next_stack_offset: 0,
            functions: HashMap::new(),
            function_order: Vec::new(),
            referenced_functions: HashSet::new(),
            instance_methods: Vec::new(),
            record_schemas,
            static_lists: Vec::new(),
            static_records: Vec::new(),
            static_maps: Vec::new(),
            static_sets: Vec::new(),
            static_lambdas: Vec::new(),
            builtin_aliases: Vec::new(),
            module_aliases: HashMap::new(),
            virtual_files: HashMap::new(),
            virtual_dirs: HashSet::new(),
            unknown_virtual_paths: HashSet::new(),
            queued_threads: Vec::new(),
            dynamic_control_depth: 0,
            mergeable_dynamic_branch_depth: 0,
        }
    }

    fn compile(mut self, expr: &Expr) -> Result<ObjectFile, Diagnostic> {
        self.predeclare_top_level_functions(expr);
        self.asm.push_reg(Reg::Rbp);
        self.asm.mov_reg_reg(Reg::Rbp, Reg::Rsp);
        self.emit_store_command_line_state();
        self.compile_top_level(expr)?;
        self.emit_queued_threads()?;
        self.emit_exit_success();
        self.emit_functions()?;
        self.emit_print_i64_runtime();
        Ok(self.asm.finish())
    }

    fn emit_store_command_line_state(&mut self) {
        self.asm.mov_data_addr(Reg::R10, self.command_line_argc);
        self.asm.load_ptr_disp32(Reg::R8, Reg::Rbp, 8);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm
            .mov_data_addr(Reg::R10, self.command_line_argv1_base);
        self.asm.lea_reg_rbp_disp8(Reg::R8, 24);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.mov_data_addr(Reg::R10, self.environment_base);
        self.asm.load_ptr_disp32(Reg::R8, Reg::Rbp, 8);
        self.asm.mov_imm64(Reg::R9, 8);
        self.asm.imul_reg_reg(Reg::R8, Reg::R9);
        self.asm.add_reg_imm32(Reg::R8, 24);
        self.asm.add_reg_reg(Reg::R8, Reg::Rbp);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
    }

    fn predeclare_top_level_functions(&mut self, expr: &Expr) {
        let expressions = match expr {
            Expr::Block { expressions, .. } => expressions.as_slice(),
            other => std::slice::from_ref(other),
        };
        let thread_aliases = top_level_thread_aliases(expressions);
        let top_level_value_names = top_level_value_names(expressions);
        for expression in expressions {
            match expression {
                Expr::DefDecl {
                    name,
                    type_params,
                    constraints,
                    params,
                    param_annotations,
                    return_annotation,
                    body,
                    ..
                } => {
                    let mut flexible_params = Vec::new();
                    let param_values = param_annotations
                        .iter()
                        .map(|annotation| {
                            let value = annotation.as_ref().and_then(native_value_from_annotation);
                            flexible_params.push(value.is_none() && annotation.is_some());
                            value.unwrap_or(NativeValue::Int)
                        })
                        .collect::<Vec<_>>();
                    let annotated_return_value = return_annotation
                        .as_ref()
                        .and_then(native_value_from_annotation);
                    let return_hint = native_value_hint_from_expr(body);
                    let inferred_return_requires_inline =
                        return_annotation.is_none() && return_hint.is_none();
                    let flexible_return = (annotated_return_value.is_none()
                        && return_annotation.is_some())
                        || inferred_return_requires_inline;
                    let contains_thread_call = expr_contains_thread_call(body, &thread_aliases);
                    let captured_top_level_names =
                        referenced_top_level_names(body, &top_level_value_names);
                    let captures_top_level_values = !captured_top_level_names.is_empty();
                    let self_recursive = {
                        let names = HashSet::from([name.clone()]);
                        expr_references_any_name(body, &names)
                    };
                    let inline_at_call_site = !type_params.is_empty()
                        || !constraints.is_empty()
                        || flexible_params.iter().any(|flexible| *flexible)
                        || flexible_return
                        || inferred_return_requires_inline
                        || contains_thread_call
                        || (captures_top_level_values && !self_recursive);
                    let return_value = annotated_return_value
                        .or(return_hint)
                        .unwrap_or(NativeValue::Int);
                    self.predeclare_function(
                        name.clone(),
                        params.clone(),
                        param_values,
                        flexible_params,
                        return_value,
                        body.as_ref().clone(),
                        inline_at_call_site,
                        flexible_return,
                        contains_thread_call,
                        captured_top_level_names,
                    );
                }
                Expr::VarDecl {
                    name,
                    value,
                    mutable: false,
                    ..
                } => {
                    if let Expr::Lambda {
                        params,
                        param_annotations,
                        body,
                        ..
                    } = value.as_ref()
                    {
                        let param_values = param_annotations
                            .iter()
                            .map(|annotation| {
                                annotation
                                    .as_ref()
                                    .and_then(native_value_from_annotation)
                                    .unwrap_or(NativeValue::Int)
                            })
                            .collect::<Vec<_>>();
                        let contains_thread_call = expr_contains_thread_call(body, &thread_aliases);
                        self.predeclare_function(
                            name.clone(),
                            params.clone(),
                            param_values,
                            vec![false; params.len()],
                            native_value_hint_from_expr(body).unwrap_or(NativeValue::Int),
                            body.as_ref().clone(),
                            true,
                            false,
                            contains_thread_call,
                            HashSet::new(),
                        );
                    }
                }
                Expr::InstanceDeclaration { methods, .. } => {
                    for method in methods {
                        if let Expr::DefDecl {
                            name,
                            params,
                            param_annotations,
                            body,
                            ..
                        } = method
                        {
                            self.instance_methods.push(NativeInstanceMethod {
                                name: name.clone(),
                                params: params.clone(),
                                param_annotations: param_annotations
                                    .iter()
                                    .map(|annotation| {
                                        annotation
                                            .as_ref()
                                            .map(|annotation| annotation.text.clone())
                                    })
                                    .collect(),
                                body: body.as_ref().clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn predeclare_function(
        &mut self,
        name: String,
        params: Vec<String>,
        param_values: Vec<NativeValue>,
        flexible_params: Vec<bool>,
        return_value: NativeValue,
        body: Expr,
        inline_at_call_site: bool,
        flexible_return: bool,
        contains_thread_call: bool,
        captured_top_level_names: HashSet<String>,
    ) {
        if self.functions.contains_key(&name) {
            return;
        }
        let label = self.asm.create_text_label();
        self.function_order.push(name.clone());
        self.functions.insert(
            name,
            NativeFunction {
                label,
                params,
                param_values,
                flexible_params,
                return_value,
                body,
                inline_at_call_site,
                flexible_return,
                contains_thread_call,
                captured_top_level_names,
            },
        );
    }

    fn compile_top_level(&mut self, expr: &Expr) -> Result<NativeValue, Diagnostic> {
        match expr {
            Expr::Block { expressions, .. } => {
                let mut last = NativeValue::Unit;
                for expression in expressions {
                    last = self.compile_top_level(expression)?;
                }
                Ok(last)
            }
            Expr::ModuleHeader { .. }
            | Expr::TypeClassDeclaration { .. }
            | Expr::InstanceDeclaration { .. }
            | Expr::DefDecl { .. } => Ok(NativeValue::Unit),
            Expr::Import { path, alias, .. } => {
                if let Some(alias) = alias
                    && Self::is_builtin_module(path)
                {
                    self.module_aliases.insert(alias.clone(), path.clone());
                }
                Ok(NativeValue::Unit)
            }
            Expr::RecordDeclaration { name, fields, .. } => {
                self.record_schemas.insert(
                    name.clone(),
                    fields.iter().map(|field| field.name.clone()).collect(),
                );
                Ok(NativeValue::Unit)
            }
            Expr::TheoremDeclaration { .. }
            | Expr::AxiomDeclaration { .. }
            | Expr::PegRuleBlock { .. } => Ok(NativeValue::Unit),
            _ => self.compile_expr(expr),
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<NativeValue, Diagnostic> {
        match expr {
            Expr::Int { value, .. } => {
                self.asm.mov_imm64(Reg::Rax, *value as u64);
                Ok(NativeValue::Int)
            }
            Expr::Double { value, kind, .. } => match kind {
                FloatLiteralKind::Float => Ok(NativeValue::StaticFloat {
                    bits: (*value as f32).to_bits(),
                }),
                FloatLiteralKind::Double => Ok(NativeValue::StaticDouble {
                    bits: value.to_bits(),
                }),
            },
            Expr::Bool { value, .. } => {
                self.asm.mov_imm64(Reg::Rax, u64::from(*value));
                Ok(NativeValue::Bool)
            }
            Expr::Null { .. } => Ok(NativeValue::Null),
            Expr::String { value, span } => {
                if value.contains("#{") {
                    let Some(value) =
                        self.static_interpolated_string_value_preserving_effects(value, *span)
                    else {
                        return self.emit_runtime_interpolated_string(value, *span);
                    };
                    return Ok(self.emit_static_string(value));
                }
                let label = self.asm.data_label_with_bytes(value.as_bytes());
                Ok(NativeValue::StaticString {
                    label,
                    len: value.len(),
                })
            }
            Expr::ListLiteral { elements, span } => {
                if let Some(values) = elements
                    .iter()
                    .map(const_int_expr)
                    .collect::<Option<Vec<_>>>()
                {
                    let label = self.asm.data_label_with_i64s(&values);
                    Ok(NativeValue::StaticIntList {
                        label,
                        len: values.len(),
                    })
                } else {
                    let elements = self.static_values_from_arguments_preserving_effects(
                        elements,
                        *span,
                        "native list element with non-static value",
                    )?;
                    let value = self.static_list_value_from_elements(elements);
                    Ok(self.emit_static_value(&value))
                }
            }
            Expr::MapLiteral { entries, span } => self.compile_map_literal(entries, *span),
            Expr::SetLiteral { elements, span } => self.compile_set_literal(elements, *span),
            Expr::RecordLiteral { fields, span } => self.compile_record_literal("", fields, *span),
            Expr::RecordConstructor {
                name,
                arguments,
                span,
            } => self.compile_record_constructor(name, arguments, *span),
            Expr::FieldAccess {
                target,
                field,
                span,
            } => self.compile_field_access(target, field, *span),
            Expr::Unit { .. } => Ok(NativeValue::Unit),
            Expr::Unary { op, expr, span } => self.compile_unary(*op, expr, *span),
            Expr::Binary { lhs, op, rhs, span } => self.compile_binary(lhs, *op, rhs, *span),
            Expr::Call {
                callee,
                arguments,
                span,
            } => self.compile_call(callee, arguments, *span),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                if let Some(value) = self.compile_statically_selected_if(
                    condition,
                    then_branch,
                    else_branch.as_deref(),
                    *span,
                )? {
                    return Ok(value);
                }
                if let Some(value) =
                    self.static_if_value(condition, then_branch, else_branch.as_deref())
                {
                    Ok(self.emit_static_value(&value))
                } else {
                    self.compile_if(condition, then_branch, else_branch.as_deref(), *span)
                }
            }
            Expr::While {
                condition,
                body,
                span,
            } => self.compile_while(condition, body, *span),
            Expr::Foreach {
                binding,
                iterable,
                body,
                span,
            } => self.compile_foreach(binding, iterable, body, *span),
            Expr::Block { expressions, .. } => {
                self.push_scope();
                let mut last = NativeValue::Unit;
                for expression in expressions {
                    last = self.compile_expr(expression)?;
                }
                if self.native_value_captures_current_scope(last)
                    || self.queued_threads_capture_current_scope()
                {
                    self.pop_scope_preserving_allocations();
                } else {
                    self.pop_scope();
                }
                Ok(last)
            }
            Expr::Cleanup { body, cleanup, .. } => {
                let body_value = self.compile_expr(body)?;
                if matches!(body_value, NativeValue::Int | NativeValue::Bool) {
                    self.push_temp_reg(Reg::Rax);
                    self.compile_expr(cleanup)?;
                    self.pop_temp_reg(Reg::Rax);
                    return Ok(body_value);
                }
                self.compile_expr(cleanup)?;
                Ok(body_value)
            }
            Expr::Lambda { params, body, .. } => {
                let thread_aliases = self.current_thread_aliases();
                let label = self.intern_static_lambda(
                    params.clone(),
                    body.as_ref().clone(),
                    self.current_static_captures(),
                    self.current_runtime_captures(),
                    expr_contains_thread_call(body, &thread_aliases),
                );
                Ok(NativeValue::StaticLambda { label })
            }
            Expr::Identifier { name, span } => {
                let Some(slot) = self.lookup_var(name) else {
                    if let Some(value) = self.static_lambda_value_for_function_name(name) {
                        return Ok(self.emit_static_value(&value));
                    }
                    if let Some(value) = self.static_builtin_value_for_identifier(name) {
                        return Ok(self.emit_static_value(&value));
                    }
                    return Err(Diagnostic::compile(
                        *span,
                        format!("undefined native variable `{name}`"),
                    ));
                };
                if matches!(slot.value, NativeValue::Int | NativeValue::Bool) {
                    self.asm.load_rbp_slot(Reg::Rax, slot.offset);
                }
                Ok(slot.value)
            }
            Expr::VarDecl {
                name,
                value,
                mutable,
                span,
                ..
            } => {
                if !mutable && let Some(alias) = self.builtin_alias_from_expr(value) {
                    let label = self.intern_builtin_alias(alias);
                    self.bind_constant(name.clone(), NativeValue::BuiltinFunction { label });
                    return Ok(NativeValue::Unit);
                }
                let compiled = self.compile_expr(value)?;
                match compiled {
                    NativeValue::Int | NativeValue::Bool => {
                        let slot = self.allocate_slot(name.clone(), compiled);
                        self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                        if static_expr_is_pure(value)
                            && let Some(value) = self.static_value_from_expr(value)
                        {
                            self.bind_static_value(name.clone(), value);
                        }
                        Ok(NativeValue::Unit)
                    }
                    NativeValue::Null
                    | NativeValue::StaticFloat { .. }
                    | NativeValue::StaticDouble { .. }
                    | NativeValue::StaticString { .. }
                    | NativeValue::RuntimeString { .. }
                    | NativeValue::RuntimeLinesList { .. }
                    | NativeValue::StaticIntList { .. }
                    | NativeValue::StaticList { .. }
                    | NativeValue::StaticRecord { .. }
                    | NativeValue::StaticMap { .. }
                    | NativeValue::StaticSet { .. }
                    | NativeValue::StaticLambda { .. }
                    | NativeValue::BuiltinFunction { .. }
                        if !mutable =>
                    {
                        self.bind_constant(name.clone(), compiled);
                        Ok(NativeValue::Unit)
                    }
                    value if *mutable && native_value_can_be_static_mutable(value) => {
                        self.bind_constant(name.clone(), value);
                        Ok(NativeValue::Unit)
                    }
                    NativeValue::Unit
                    | NativeValue::Null
                    | NativeValue::StaticFloat { .. }
                    | NativeValue::StaticDouble { .. }
                    | NativeValue::StaticString { .. }
                    | NativeValue::RuntimeString { .. }
                    | NativeValue::RuntimeLinesList { .. }
                    | NativeValue::StaticIntList { .. }
                    | NativeValue::StaticList { .. }
                    | NativeValue::StaticRecord { .. }
                    | NativeValue::StaticMap { .. }
                    | NativeValue::StaticSet { .. }
                    | NativeValue::StaticLambda { .. }
                    | NativeValue::BuiltinFunction { .. } => Err(unsupported(
                        *span,
                        "native mutable binding for this value type",
                    )),
                }
            }
            Expr::Assign { name, value, span } => {
                let slot = self.lookup_var(name).ok_or_else(|| {
                    Diagnostic::compile(*span, format!("undefined native variable `{name}`"))
                })?;
                if native_value_can_be_static_mutable(slot.value) {
                    if self.dynamic_control_depth > 0 && self.mergeable_dynamic_branch_depth == 0 {
                        return Err(unsupported(
                            *span,
                            "native static aggregate assignment inside dynamic control flow",
                        ));
                    }
                    let compiled = self.compile_expr(value)?;
                    if !native_value_can_be_static_mutable(compiled) {
                        return Err(unsupported(
                            *span,
                            "native static aggregate assignment with non-static value",
                        ));
                    }
                    self.assign_var_value(name, compiled);
                    if let Some(value) = self.static_value_from_native(compiled) {
                        self.assign_static_value(name, value);
                    } else {
                        self.remove_static_value(name);
                    }
                    return Ok(compiled);
                }
                if !matches!(slot.value, NativeValue::Int | NativeValue::Bool) {
                    return Err(unsupported(*span, "native assignment to this value type"));
                }
                let compiled = self.compile_expr(value)?;
                if compiled != slot.value {
                    return Err(unsupported(
                        *span,
                        "native assignment with changed value type",
                    ));
                }
                self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                if self.dynamic_control_depth > 0 {
                    self.remove_static_value(name);
                } else if static_expr_is_pure(value)
                    && let Some(value) = self.static_value_from_expr(value)
                {
                    self.assign_static_value(name, value);
                } else {
                    self.remove_static_value(name);
                }
                Ok(compiled)
            }
            Expr::DefDecl { span, .. }
            | Expr::ModuleHeader { span, .. }
            | Expr::Import { span, .. }
            | Expr::RecordDeclaration { span, .. }
            | Expr::TypeClassDeclaration { span, .. }
            | Expr::InstanceDeclaration { span, .. }
            | Expr::TheoremDeclaration { span, .. }
            | Expr::AxiomDeclaration { span, .. } => {
                Err(unsupported(*span, native_feature_name(expr)))
            }
            Expr::PegRuleBlock { .. } => Ok(NativeValue::Unit),
        }
    }

    fn compile_unary(
        &mut self,
        op: UnaryOp,
        expr: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let value = self.compile_expr(expr)?;
        match (op, value) {
            (UnaryOp::Plus, NativeValue::Int) => Ok(NativeValue::Int),
            (UnaryOp::Plus, NativeValue::StaticFloat { bits }) => {
                Ok(NativeValue::StaticFloat { bits })
            }
            (UnaryOp::Plus, NativeValue::StaticDouble { bits }) => {
                Ok(NativeValue::StaticDouble { bits })
            }
            (UnaryOp::Minus, NativeValue::Int) => {
                self.asm.neg_reg(Reg::Rax);
                Ok(NativeValue::Int)
            }
            (UnaryOp::Minus, NativeValue::StaticFloat { bits }) => Ok(NativeValue::StaticFloat {
                bits: (-f32::from_bits(bits)).to_bits(),
            }),
            (UnaryOp::Minus, NativeValue::StaticDouble { bits }) => Ok(NativeValue::StaticDouble {
                bits: (-f64::from_bits(bits)).to_bits(),
            }),
            (UnaryOp::Not, NativeValue::Bool) => {
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.setcc_al(Condition::Equal);
                self.asm.movzx_rax_al();
                Ok(NativeValue::Bool)
            }
            _ => Err(unsupported(span, "native unary operation for this type")),
        }
    }

    fn compile_binary(
        &mut self,
        lhs: &Expr,
        op: BinaryOp,
        rhs: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if matches!(op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
            return self.compile_logical(lhs, op, rhs, span);
        }
        if op == BinaryOp::Add
            && (self.expr_may_yield_runtime_string(lhs)
                || self.expr_may_yield_runtime_string(rhs)
                || self.expr_may_yield_static_string(lhs)
                || self.expr_may_yield_static_string(rhs))
        {
            if let Some(value) = self.static_string_concat_text(lhs, rhs) {
                return Ok(self.emit_static_string(value));
            }
            return self.compile_runtime_string_concat(lhs, rhs, span);
        }
        if op == BinaryOp::Add
            && let Some(value) = self.static_string_concat_text(lhs, rhs)
        {
            return Ok(self.emit_static_string(value));
        }
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && let Some(equal) = self.static_equality_from_exprs(lhs, rhs)
        {
            self.asm
                .mov_imm64(Reg::Rax, u64::from(equal == (op == BinaryOp::Equal)));
            return Ok(NativeValue::Bool);
        }
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && let Some(equal) =
                self.static_equality_from_exprs_preserving_effects(lhs, rhs, span)?
        {
            self.asm
                .mov_imm64(Reg::Rax, u64::from(equal == (op == BinaryOp::Equal)));
            return Ok(NativeValue::Bool);
        }
        if let Some(value) = self.static_numeric_binary_from_exprs(lhs, op, rhs) {
            return Ok(self.emit_static_value(&value));
        }
        if let Some(value) =
            self.static_numeric_binary_from_exprs_preserving_effects(lhs, op, rhs, span)?
        {
            return Ok(self.emit_static_value(&value));
        }
        let lhs_value = self.compile_expr(lhs)?;
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && matches!(lhs_value, NativeValue::Int | NativeValue::Bool)
        {
            self.push_temp_reg(Reg::Rax);
            let rhs_value = self.compile_expr(rhs)?;
            if rhs_value != lhs_value {
                return Err(unsupported(
                    span,
                    "native equality for values with different types",
                ));
            }
            self.pop_temp_reg(Reg::Rcx);
            self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
            self.asm.setcc_al(if op == BinaryOp::Equal {
                Condition::Equal
            } else {
                Condition::NotEqual
            });
            self.asm.movzx_rax_al();
            return Ok(NativeValue::Bool);
        }
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && let Some(lhs_string) = self.native_string_ref(lhs_value)
        {
            let rhs_value = self.compile_expr(rhs)?;
            let Some(rhs_string) = self.native_string_ref(rhs_value) else {
                return Err(unsupported(
                    span,
                    "native equality for values with different types",
                ));
            };
            self.emit_native_string_equality(lhs_string, rhs_string);
            if op == BinaryOp::NotEqual {
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.setcc_al(Condition::Equal);
                self.asm.movzx_rax_al();
            }
            return Ok(NativeValue::Bool);
        }
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && let NativeValue::RuntimeLinesList { data, len } = lhs_value
        {
            let rhs_value = self.compile_expr(rhs)?;
            match rhs_value {
                NativeValue::StaticList { label } => self.emit_runtime_lines_equal_static_list(
                    NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    },
                    label,
                    span,
                )?,
                NativeValue::RuntimeLinesList {
                    data: rhs_data,
                    len: rhs_len,
                } => self.emit_runtime_lines_equal_runtime_lines(
                    NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    },
                    NativeStringRef {
                        data: rhs_data,
                        len: NativeStringLen::Runtime(rhs_len),
                    },
                ),
                _ => {
                    return Err(unsupported(
                        span,
                        "native equality for runtime lines list and this value type",
                    ));
                }
            };
            if op == BinaryOp::NotEqual {
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.setcc_al(Condition::Equal);
                self.asm.movzx_rax_al();
            }
            return Ok(NativeValue::Bool);
        }
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && let NativeValue::StaticList { label } = lhs_value
        {
            let rhs_value = self.compile_expr(rhs)?;
            let NativeValue::RuntimeLinesList { data, len } = rhs_value else {
                let Some(equal) =
                    self.static_values_equal_user(NativeValue::StaticList { label }, rhs_value)
                else {
                    return Err(unsupported(
                        span,
                        "native equality for values with different types",
                    ));
                };
                self.asm
                    .mov_imm64(Reg::Rax, u64::from(equal == (op == BinaryOp::Equal)));
                return Ok(NativeValue::Bool);
            };
            self.emit_runtime_lines_equal_static_list(
                NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                },
                label,
                span,
            )?;
            if op == BinaryOp::NotEqual {
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.setcc_al(Condition::Equal);
                self.asm.movzx_rax_al();
            }
            return Ok(NativeValue::Bool);
        }
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            && matches!(
                lhs_value,
                NativeValue::Null
                    | NativeValue::Unit
                    | NativeValue::StaticFloat { .. }
                    | NativeValue::StaticDouble { .. }
                    | NativeValue::StaticString { .. }
                    | NativeValue::RuntimeString { .. }
                    | NativeValue::StaticIntList { .. }
                    | NativeValue::StaticRecord { .. }
                    | NativeValue::StaticMap { .. }
                    | NativeValue::StaticSet { .. }
                    | NativeValue::StaticLambda { .. }
                    | NativeValue::BuiltinFunction { .. }
            )
        {
            let rhs_value = self.compile_expr(rhs)?;
            let Some(equal) = self.static_values_equal_user(lhs_value, rhs_value) else {
                return Err(unsupported(
                    span,
                    "native equality for values with different types",
                ));
            };
            self.asm
                .mov_imm64(Reg::Rax, u64::from(equal == (op == BinaryOp::Equal)));
            return Ok(NativeValue::Bool);
        }
        if lhs_value != NativeValue::Int {
            return Err(unsupported(span, "native binary operation for non-Int lhs"));
        }
        self.push_temp_reg(Reg::Rax);
        let rhs_value = self.compile_expr(rhs)?;
        if rhs_value != NativeValue::Int {
            return Err(unsupported(span, "native binary operation for non-Int rhs"));
        }
        self.pop_temp_reg(Reg::Rcx);
        match op {
            BinaryOp::Add => self.asm.add_reg_reg(Reg::Rax, Reg::Rcx),
            BinaryOp::Subtract => {
                self.asm.sub_reg_reg(Reg::Rcx, Reg::Rax);
                self.asm.mov_reg_reg(Reg::Rax, Reg::Rcx);
            }
            BinaryOp::Multiply => self.asm.imul_reg_reg(Reg::Rax, Reg::Rcx),
            BinaryOp::Divide => {
                self.asm.mov_reg_reg(Reg::Rbx, Reg::Rax);
                self.asm.mov_reg_reg(Reg::Rax, Reg::Rcx);
                self.asm.cqo();
                self.asm.idiv_reg(Reg::Rbx);
            }
            BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::Equal
            | BinaryOp::NotEqual => {
                self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
                let condition = match op {
                    BinaryOp::Less => Condition::Less,
                    BinaryOp::LessEqual => Condition::LessEqual,
                    BinaryOp::Greater => Condition::Greater,
                    BinaryOp::GreaterEqual => Condition::GreaterEqual,
                    BinaryOp::Equal => Condition::Equal,
                    BinaryOp::NotEqual => Condition::NotEqual,
                    _ => unreachable!("comparison op matched above"),
                };
                self.asm.setcc_al(condition);
                self.asm.movzx_rax_al();
                return Ok(NativeValue::Bool);
            }
            BinaryOp::BitAnd => self.asm.and_reg_reg(Reg::Rax, Reg::Rcx),
            BinaryOp::BitOr => self.asm.or_reg_reg(Reg::Rax, Reg::Rcx),
            BinaryOp::BitXor => self.asm.xor_reg_reg(Reg::Rax, Reg::Rcx),
            BinaryOp::LogicalAnd | BinaryOp::LogicalOr => unreachable!("handled above"),
        }
        Ok(NativeValue::Int)
    }

    fn compile_logical(
        &mut self,
        lhs: &Expr,
        op: BinaryOp,
        rhs: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let end = self.asm.create_text_label();
        let short = self.asm.create_text_label();
        let before_lhs_static_scopes = self.static_scopes.clone();
        let lhs_preview = self.preview_static_value_after_effectful_eval(lhs);
        self.static_scopes = before_lhs_static_scopes;
        let lhs_value = self.compile_expr(lhs)?;
        if lhs_value != NativeValue::Bool {
            return Err(unsupported(span, "native logical lhs for non-Bool"));
        }
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        match op {
            BinaryOp::LogicalAnd => self.asm.jcc_label(Condition::Equal, short),
            BinaryOp::LogicalOr => self.asm.jcc_label(Condition::NotEqual, short),
            _ => unreachable!("logical op expected"),
        }
        let before_rhs_static_scopes = self.static_scopes.clone();
        let short_circuits = matches!(
            (op, lhs_preview.clone()),
            (BinaryOp::LogicalAnd, Some(StaticValue::Bool(false)))
                | (BinaryOp::LogicalOr, Some(StaticValue::Bool(true)))
        );
        if short_circuits {
            self.asm.bind_text_label(short);
            self.asm.mov_imm64(
                Reg::Rax,
                match op {
                    BinaryOp::LogicalAnd => 0,
                    BinaryOp::LogicalOr => 1,
                    _ => unreachable!("logical op expected"),
                },
            );
            self.asm.bind_text_label(end);
            self.static_scopes = before_rhs_static_scopes;
            return Ok(NativeValue::Bool);
        }
        let rhs_value = self.compile_expr(rhs)?;
        if rhs_value != NativeValue::Bool {
            return Err(unsupported(span, "native logical rhs for non-Bool"));
        }
        let after_rhs_static_scopes = self.static_scopes.clone();
        self.asm.jmp_label(end);
        self.asm.bind_text_label(short);
        self.asm.mov_imm64(
            Reg::Rax,
            match op {
                BinaryOp::LogicalAnd => 0,
                BinaryOp::LogicalOr => 1,
                _ => unreachable!("logical op expected"),
            },
        );
        self.asm.bind_text_label(end);
        self.static_scopes = match (op, lhs_preview) {
            (BinaryOp::LogicalAnd, Some(StaticValue::Bool(true)))
            | (BinaryOp::LogicalOr, Some(StaticValue::Bool(false))) => after_rhs_static_scopes,
            _ => {
                self.conditional_static_scopes(&before_rhs_static_scopes, &after_rhs_static_scopes)
            }
        };
        Ok(NativeValue::Bool)
    }

    fn compile_call(
        &mut self,
        callee: &Expr,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if let Expr::Call {
            callee: middle_callee,
            arguments: initial_arguments,
            ..
        } = callee
            && let Expr::Call {
                callee: root_callee,
                arguments: list_arguments,
                ..
            } = middle_callee.as_ref()
            && let Expr::Identifier { name, .. } = root_callee.as_ref()
            && self.builtin_name_for_identifier(name) == "foldLeft"
        {
            return self.compile_static_fold_left(
                list_arguments,
                initial_arguments,
                arguments,
                span,
            );
        }
        if let Expr::Call {
            callee: middle_callee,
            arguments: initial_arguments,
            ..
        } = callee
            && let Expr::FieldAccess { target, field, .. } = middle_callee.as_ref()
            && field == "foldLeft"
        {
            return self.compile_static_fold_left(
                std::slice::from_ref(target),
                initial_arguments,
                arguments,
                span,
            );
        }
        if let Expr::Call {
            callee: nested_callee,
            arguments: expected_arguments,
            ..
        } = callee
            && let Expr::Identifier { name, .. } = nested_callee.as_ref()
        {
            let name = self.builtin_name_for_identifier(name);
            if name == "assertResult" {
                return self.compile_assert_result(expected_arguments, arguments, span);
            }
            if name == "cons" {
                return self.compile_cons(expected_arguments, arguments, span);
            }
            if name == "map" {
                return self.compile_static_map(expected_arguments, arguments, span);
            }
            match name.as_str() {
                "Map#containsKey" | "containsKey" => {
                    return self.compile_static_map_contains_key(
                        expected_arguments,
                        arguments,
                        span,
                    );
                }
                "Map#containsValue" | "containsValue" => {
                    return self.compile_static_map_contains_value(
                        expected_arguments,
                        arguments,
                        span,
                    );
                }
                "Map#get" | "get" => {
                    return self.compile_static_map_get(expected_arguments, arguments, span);
                }
                "Set#contains" | "contains" => {
                    return self.compile_static_set_contains(expected_arguments, arguments, span);
                }
                _ => {}
            }
        }
        if let Expr::FieldAccess { target, field, .. } = callee {
            return self.compile_static_method_call(target, field, arguments, span);
        }
        if let Some(value) = self.compile_static_curried_fold_like_call(callee, arguments, span)? {
            return Ok(value);
        }
        if let Expr::Call {
            callee: middle_callee,
            arguments: initial_arguments,
            ..
        } = callee
            && let Expr::Call {
                callee: nested_callee,
                arguments: list_arguments,
                ..
            } = middle_callee.as_ref()
            && !matches!(nested_callee.as_ref(), Expr::Identifier { .. })
        {
            let callee_value = self.compile_expr(nested_callee)?;
            let NativeValue::BuiltinFunction { label } = callee_value else {
                return Err(unsupported(
                    span,
                    "native curried builtin function value call",
                ));
            };
            let name =
                self.builtin_aliases.get(label.0).cloned().ok_or_else(|| {
                    unsupported(span, "native curried builtin function value call")
                })?;
            if name == "foldLeft" {
                return self.compile_static_fold_left(
                    list_arguments,
                    initial_arguments,
                    arguments,
                    span,
                );
            }
            return Err(unsupported(
                span,
                "native curried builtin function value call",
            ));
        }
        if let Expr::Call {
            callee: nested_callee,
            arguments: expected_arguments,
            ..
        } = callee
            && !matches!(nested_callee.as_ref(), Expr::Identifier { .. })
        {
            let callee_value = self.compile_expr(nested_callee)?;
            let NativeValue::BuiltinFunction { label } = callee_value else {
                return Err(unsupported(
                    span,
                    "native curried builtin function value call",
                ));
            };
            let name =
                self.builtin_aliases.get(label.0).cloned().ok_or_else(|| {
                    unsupported(span, "native curried builtin function value call")
                })?;
            match name.as_str() {
                "assertResult" => {
                    return self.compile_assert_result(expected_arguments, arguments, span);
                }
                "cons" => {
                    return self.compile_cons(expected_arguments, arguments, span);
                }
                "map" => {
                    return self.compile_static_map(expected_arguments, arguments, span);
                }
                "Map#containsKey" | "containsKey" => {
                    return self.compile_static_map_contains_key(
                        expected_arguments,
                        arguments,
                        span,
                    );
                }
                "Map#containsValue" | "containsValue" => {
                    return self.compile_static_map_contains_value(
                        expected_arguments,
                        arguments,
                        span,
                    );
                }
                "Map#get" | "get" => {
                    return self.compile_static_map_get(expected_arguments, arguments, span);
                }
                "Set#contains" | "contains" => {
                    return self.compile_static_set_contains(expected_arguments, arguments, span);
                }
                _ => {
                    return Err(unsupported(
                        span,
                        "native curried builtin function value call",
                    ));
                }
            }
        }
        if !matches!(callee, Expr::Identifier { .. })
            && let Some(argument_values) = arguments
                .iter()
                .map(|argument| self.static_value_from_pure_expr(argument))
                .collect::<Option<Vec<_>>>()
            && let Some(value) = self.static_apply_callable_value(callee, argument_values)
        {
            return Ok(self.emit_static_value(&value));
        }
        if let Expr::Lambda { params, body, .. } = callee {
            return self.compile_inline_lambda_call(params, body, arguments, span);
        }
        let Expr::Identifier {
            name: callee_name, ..
        } = callee
        else {
            let callee_value = self.compile_expr(callee)?;
            match callee_value {
                NativeValue::StaticLambda { label } => {
                    return self.compile_static_lambda_inline_call(label, arguments, span);
                }
                NativeValue::BuiltinFunction { label } => {
                    let name =
                        self.builtin_aliases.get(label.0).cloned().ok_or_else(|| {
                            unsupported(span, "native builtin function value call")
                        })?;
                    if let Some(value) =
                        self.compile_builtin_function_value_call(&name, arguments, span)?
                    {
                        return Ok(value);
                    }
                    let arguments = self.static_values_from_arguments_preserving_effects(
                        arguments,
                        span,
                        "native builtin function value argument",
                    )?;
                    if let Some(value) =
                        self.static_call_value_by_name_with_values(&name, &arguments)
                    {
                        return Ok(self.emit_static_value(&value));
                    }
                    return Err(unsupported(span, "native builtin function value call"));
                }
                _ => {}
            }
            return Err(unsupported(span, "native non-identifier call"));
        };
        if let Some(StaticValue::StaticLambda { label }) = self.lookup_static_value(callee_name) {
            let lambda_body_is_pure = self.static_lambdas.get(label.0).is_some_and(|lambda| {
                !lambda.contains_thread_call
                    && static_expr_is_pure(&lambda.body)
                    && !self.lambda_uses_runtime_captures(lambda)
            });
            let static_arguments = arguments.iter().all(|argument| {
                self.preview_static_value_after_effectful_eval(argument)
                    .is_some()
            });
            if lambda_body_is_pure && static_arguments {
                let arguments = self.static_values_from_arguments_preserving_effects(
                    arguments,
                    span,
                    "native static lambda argument",
                )?;
                let value = self.compile_static_lambda_with_static_arguments_preserving_effects(
                    label,
                    arguments,
                    span,
                    "native static lambda body",
                )?;
                return Ok(self.emit_static_value(&value));
            }
        }
        if let Some(StaticValue::StaticLambda { label }) = self.lookup_static_value(callee_name) {
            return self.compile_static_lambda_inline_call(label, arguments, span);
        }
        if self.functions.contains_key(callee_name) {
            return self.compile_function_call_by_name(callee_name, arguments, span);
        }
        let name = self.builtin_name_for_identifier(callee_name);
        match name.as_str() {
            "println" | "printlnError" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let fd = if name == "println" { 1 } else { 2 };
                self.emit_print_expr_line(fd, &arguments[0], span)?;
                Ok(NativeValue::Unit)
            }
            "ToDo" => {
                if !arguments.is_empty() {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 0 arguments but got {}", arguments.len()),
                    ));
                }
                self.emit_todo_failed(span);
                Ok(NativeValue::Unit)
            }
            "sleep" => self.compile_sleep(arguments, span),
            "thread" => self.compile_thread(arguments, span),
            "stopwatch" => self.compile_stopwatch(arguments, span),
            "assert" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let value = self.compile_expr(&arguments[0])?;
                if value != NativeValue::Bool {
                    return Err(unsupported(span, "native assert for non-Bool"));
                }
                let ok = self.asm.create_text_label();
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.jcc_label(Condition::NotEqual, ok);
                self.emit_assertion_failed(span);
                self.asm.bind_text_label(ok);
                Ok(NativeValue::Unit)
            }
            "toString" | "substring" | "at" | "matches" | "split" | "trim" | "trimLeft"
            | "trimRight" | "replace" | "replaceAll" | "toLowerCase" | "toUpperCase"
            | "startsWith" | "endsWith" | "isEmptyString" | "indexOf" | "lastIndexOf"
            | "length" | "repeat" | "reverse" => {
                self.compile_static_string_helper(name.as_str(), arguments, span)
            }
            "join" => self.compile_static_join(arguments, span),
            "contains" => self.compile_static_contains_direct(arguments, span),
            "double" | "sqrt" | "int" | "floor" | "ceil" | "abs" => {
                self.compile_numeric_helper(name.as_str(), arguments, span)
            }
            "size" | "Map#size" | "Set#size" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let value = self.compile_expr(&arguments[0])?;
                if name == "size"
                    && let NativeValue::RuntimeLinesList { data, len } = value
                {
                    self.emit_runtime_lines_count(NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    });
                    return Ok(NativeValue::Int);
                }
                let len = match (name.as_str(), value) {
                    ("size", NativeValue::StaticIntList { len, .. }) => len,
                    ("size", NativeValue::StaticList { label }) => {
                        self.static_lists[label.0].elements.len()
                    }
                    ("size", NativeValue::StaticMap { label }) => {
                        self.static_maps[label.0].entries.len()
                    }
                    ("size", NativeValue::StaticSet { label }) => {
                        self.static_sets[label.0].elements.len()
                    }
                    ("Map#size", NativeValue::StaticMap { label }) => {
                        self.static_maps[label.0].entries.len()
                    }
                    ("Set#size", NativeValue::StaticSet { label }) => {
                        self.static_sets[label.0].elements.len()
                    }
                    ("Map#size", _) => {
                        return Err(unsupported(span, "native Map#size for non-static map"));
                    }
                    ("Set#size", _) => {
                        return Err(unsupported(span, "native Set#size for non-static set"));
                    }
                    _ => return Err(unsupported(span, "native size for this value type")),
                };
                self.asm.mov_imm64(Reg::Rax, len as u64);
                Ok(NativeValue::Int)
            }
            "isEmpty" | "Map#isEmpty" | "Set#isEmpty" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let value = self.compile_expr(&arguments[0])?;
                if name == "isEmpty"
                    && let NativeValue::RuntimeLinesList { data, len } = value
                {
                    self.emit_runtime_lines_count(NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    });
                    self.asm.cmp_reg_imm8(Reg::Rax, 0);
                    self.asm.setcc_al(Condition::Equal);
                    self.asm.movzx_rax_al();
                    return Ok(NativeValue::Bool);
                }
                let is_empty = match (name.as_str(), value) {
                    ("isEmpty", NativeValue::StaticIntList { len, .. }) => len == 0,
                    ("isEmpty", NativeValue::StaticList { label }) => {
                        self.static_lists[label.0].elements.is_empty()
                    }
                    ("isEmpty", NativeValue::StaticMap { label })
                    | ("Map#isEmpty", NativeValue::StaticMap { label }) => {
                        self.static_maps[label.0].entries.is_empty()
                    }
                    ("isEmpty", NativeValue::StaticSet { label })
                    | ("Set#isEmpty", NativeValue::StaticSet { label }) => {
                        self.static_sets[label.0].elements.is_empty()
                    }
                    ("Map#isEmpty", _) => {
                        return Err(unsupported(span, "native Map#isEmpty for non-static map"));
                    }
                    ("Set#isEmpty", _) => {
                        return Err(unsupported(span, "native Set#isEmpty for non-static set"));
                    }
                    _ => return Err(unsupported(span, "native isEmpty for this value type")),
                };
                self.asm.mov_imm64(Reg::Rax, u64::from(is_empty));
                Ok(NativeValue::Bool)
            }
            "head" => self.compile_static_head(arguments, span),
            "tail" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let value = self.compile_expr(&arguments[0])?;
                match value {
                    NativeValue::StaticIntList { label, len } => {
                        let values = self.asm.i64s_for_label(label, len);
                        let tail = values.get(1..).unwrap_or_default();
                        let label = self.asm.data_label_with_i64s(tail);
                        Ok(NativeValue::StaticIntList {
                            label,
                            len: tail.len(),
                        })
                    }
                    NativeValue::StaticList { label } => {
                        let elements = self
                            .static_lists
                            .get(label.0)
                            .map(|list| list.elements.iter().skip(1).cloned().collect())
                            .unwrap_or_default();
                        let label = self.intern_static_list(elements);
                        Ok(NativeValue::StaticList { label })
                    }
                    NativeValue::RuntimeLinesList { data, len } => Ok(self
                        .emit_runtime_lines_tail(NativeStringRef {
                            data,
                            len: NativeStringLen::Runtime(len),
                        })),
                    _ => Err(unsupported(span, "native tail for non-static list")),
                }
            }
            "cons" if arguments.len() == 2 => self.compile_cons(
                std::slice::from_ref(&arguments[0]),
                std::slice::from_ref(&arguments[1]),
                span,
            ),
            "Map#containsKey" | "containsKey" => {
                self.compile_static_map_contains_key_direct(arguments, span)
            }
            "Map#containsValue" | "containsValue" => {
                self.compile_static_map_contains_value_direct(arguments, span)
            }
            "Map#get" | "get" => self.compile_static_map_get_direct(arguments, span),
            "Set#contains" => self.compile_static_set_contains_direct(arguments, span),
            "FileOutput#write" => self.compile_file_output_write(arguments, false, span),
            "FileOutput#append" => self.compile_file_output_write(arguments, true, span),
            "FileOutput#writeLines" => self.compile_file_output_write_lines(arguments, span),
            "FileOutput#exists" => self.compile_file_output_exists(arguments, span),
            "FileOutput#delete" => self.compile_file_output_delete(arguments, span),
            "StandardInput#all" => self.compile_standard_input_all(arguments, span),
            "StandardInput#lines" => self.compile_standard_input_lines(arguments, span),
            "Environment#vars" => self.compile_environment_vars(arguments, span),
            "Environment#get" => self.compile_environment_get(arguments, span),
            "Environment#exists" => self.compile_environment_exists(arguments, span),
            "CommandLine#args" => self.compile_command_line_args(arguments, span),
            "Process#exit" => self.compile_process_exit(arguments, span),
            "FileInput#open" => self.compile_file_input_open(arguments, span),
            "FileInput#all" => self.compile_file_input_all(arguments, span),
            "FileInput#lines" => self.compile_file_input_lines(arguments, span),
            "FileInput#readAll" => self.compile_file_input_all(arguments, span),
            "FileInput#readLines" => self.compile_file_input_lines(arguments, span),
            "Dir#current" => self.compile_dir_current(arguments, span),
            "Dir#home" => self.compile_dir_home(arguments, span),
            "Dir#temp" => self.compile_dir_temp(arguments, span),
            "Dir#exists" => self.compile_dir_exists(arguments, span),
            "Dir#mkdir" => self.compile_dir_mkdir(arguments, false, span),
            "Dir#mkdirs" => self.compile_dir_mkdir(arguments, true, span),
            "Dir#isDirectory" => self.compile_dir_is_directory(arguments, span),
            "Dir#isFile" => self.compile_dir_is_file(arguments, span),
            "Dir#list" => self.compile_dir_list(arguments, false, span),
            "Dir#listFull" => self.compile_dir_list(arguments, true, span),
            "Dir#delete" => self.compile_dir_delete(arguments, span),
            "Dir#copy" => self.compile_dir_copy(arguments, span),
            "Dir#move" => self.compile_dir_move(arguments, span),
            name if self.functions.contains_key(name) => {
                self.compile_function_call_by_name(name, arguments, span)
            }
            name => {
                if let Some(value) = self.static_instance_method_call_value(name, arguments) {
                    Ok(self.emit_static_value(&value))
                } else {
                    Err(unsupported(span, "native call target"))
                }
            }
        }
    }

    fn compile_function_call_by_name(
        &mut self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let function = self
            .functions
            .get(name)
            .expect("function existence was checked")
            .clone();
        if !function.contains_thread_call
            && let Some(value) = self.static_function_call_value(&function, arguments)
        {
            return Ok(self.emit_static_value(&value));
        }
        if function.inline_at_call_site {
            return self.compile_inline_function_call(&function, arguments, span);
        }
        self.referenced_functions.insert(name.to_string());
        if arguments.len() != function.params.len() {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "{name} expects {} arguments but got {}",
                    function.params.len(),
                    arguments.len()
                ),
            ));
        }
        for (argument, expected_value) in arguments.iter().zip(function.param_values.iter()) {
            let value = self.compile_expr(argument)?;
            if value != *expected_value {
                return Err(unsupported(
                    span,
                    "native function argument for this value type",
                ));
            }
            self.push_temp_reg(Reg::Rax);
        }
        let arg_regs = argument_registers(arguments.len());
        let pass_on_stack = arguments.len() > arg_regs.len();
        if !pass_on_stack {
            for reg in arg_regs.into_iter().rev() {
                self.pop_temp_reg(reg);
            }
        }
        self.asm.call_label(function.label);
        if pass_on_stack {
            self.asm
                .add_reg_imm32(Reg::Rsp, (arguments.len() * 8) as i32);
            self.release_temp_stack(arguments.len() * 8);
        }
        Ok(function.return_value)
    }

    fn compile_builtin_function_value_call(
        &mut self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<Option<NativeValue>, Diagnostic> {
        match name {
            "println" | "printlnError" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let fd = if name == "println" { 1 } else { 2 };
                self.emit_print_expr_line(fd, &arguments[0], span)?;
                Ok(Some(NativeValue::Unit))
            }
            "ToDo" => {
                if !arguments.is_empty() {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 0 arguments but got {}", arguments.len()),
                    ));
                }
                self.emit_todo_failed(span);
                Ok(Some(NativeValue::Unit))
            }
            "sleep" => self.compile_sleep(arguments, span).map(Some),
            "thread" => self.compile_thread(arguments, span).map(Some),
            "stopwatch" => self.compile_stopwatch(arguments, span).map(Some),
            "head" => self.compile_static_head(arguments, span).map(Some),
            "FileOutput#write" => self
                .compile_file_output_write(arguments, false, span)
                .map(Some),
            "FileOutput#append" => self
                .compile_file_output_write(arguments, true, span)
                .map(Some),
            "FileOutput#writeLines" => self
                .compile_file_output_write_lines(arguments, span)
                .map(Some),
            "FileOutput#exists" => self.compile_file_output_exists(arguments, span).map(Some),
            "FileOutput#delete" => self.compile_file_output_delete(arguments, span).map(Some),
            "StandardInput#all" => self.compile_standard_input_all(arguments, span).map(Some),
            "StandardInput#lines" => self.compile_standard_input_lines(arguments, span).map(Some),
            "Environment#vars" => self.compile_environment_vars(arguments, span).map(Some),
            "Environment#get" => self.compile_environment_get(arguments, span).map(Some),
            "Environment#exists" => self.compile_environment_exists(arguments, span).map(Some),
            "CommandLine#args" => self.compile_command_line_args(arguments, span).map(Some),
            "Process#exit" => self.compile_process_exit(arguments, span).map(Some),
            "FileInput#open" => self.compile_file_input_open(arguments, span).map(Some),
            "FileInput#all" | "FileInput#readAll" => {
                self.compile_file_input_all(arguments, span).map(Some)
            }
            "FileInput#lines" | "FileInput#readLines" => {
                self.compile_file_input_lines(arguments, span).map(Some)
            }
            "Dir#current" => self.compile_dir_current(arguments, span).map(Some),
            "Dir#home" => self.compile_dir_home(arguments, span).map(Some),
            "Dir#temp" => self.compile_dir_temp(arguments, span).map(Some),
            "Dir#exists" => self.compile_dir_exists(arguments, span).map(Some),
            "Dir#mkdir" => self.compile_dir_mkdir(arguments, false, span).map(Some),
            "Dir#mkdirs" => self.compile_dir_mkdir(arguments, true, span).map(Some),
            "Dir#isDirectory" => self.compile_dir_is_directory(arguments, span).map(Some),
            "Dir#isFile" => self.compile_dir_is_file(arguments, span).map(Some),
            "Dir#list" => self.compile_dir_list(arguments, false, span).map(Some),
            "Dir#listFull" => self.compile_dir_list(arguments, true, span).map(Some),
            "Dir#delete" => self.compile_dir_delete(arguments, span).map(Some),
            "Dir#copy" => self.compile_dir_copy(arguments, span).map(Some),
            "Dir#move" => self.compile_dir_move(arguments, span).map(Some),
            "assert" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("{name} expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let value = self.compile_expr(&arguments[0])?;
                if value != NativeValue::Bool {
                    return Err(unsupported(span, "native assert for non-Bool"));
                }
                let ok = self.asm.create_text_label();
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.jcc_label(Condition::NotEqual, ok);
                self.emit_assertion_failed(span);
                self.asm.bind_text_label(ok);
                Ok(Some(NativeValue::Unit))
            }
            _ => Ok(None),
        }
    }

    fn compile_assert_result(
        &mut self,
        expected_arguments: &[Expr],
        actual_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if expected_arguments.len() != 1 || actual_arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                "assertResult expects one expected value and one actual value",
            ));
        }
        if let (Some(expected), Some(actual)) = (
            self.static_value_from_pure_expr(&expected_arguments[0]),
            self.static_value_from_pure_expr(&actual_arguments[0]),
        ) {
            if !self.static_value_equal_user(&expected, &actual) {
                self.emit_assert_result_failed_static(span, &expected, &actual);
            }
            return Ok(NativeValue::Unit);
        }
        let expected = self.compile_expr(&expected_arguments[0])?;
        match expected {
            NativeValue::Int | NativeValue::Bool => {
                self.push_temp_reg(Reg::Rax);
                let actual = self.compile_expr(&actual_arguments[0])?;
                if actual != expected {
                    return Err(unsupported(
                        span,
                        "native assertResult for values with different types",
                    ));
                }
                self.pop_temp_reg(Reg::Rcx);
                let ok = self.asm.create_text_label();
                self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
                self.asm.jcc_label(Condition::Equal, ok);
                self.emit_assert_result_failed_runtime(span, expected);
                self.asm.bind_text_label(ok);
                Ok(NativeValue::Unit)
            }
            NativeValue::StaticString { .. } | NativeValue::RuntimeString { .. } => {
                let expected_string = self
                    .native_string_ref(expected)
                    .expect("string value should expose native string ref");
                let actual = self.compile_expr(&actual_arguments[0])?;
                let Some(actual_string) = self.native_string_ref(actual) else {
                    return Err(unsupported(
                        span,
                        "native assertResult for values with different types",
                    ));
                };
                self.emit_native_string_equality(expected_string, actual_string);
                let ok = self.asm.create_text_label();
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.jcc_label(Condition::NotEqual, ok);
                self.emit_runtime_error(span, "assertResult failed");
                self.asm.bind_text_label(ok);
                Ok(NativeValue::Unit)
            }
            NativeValue::StaticFloat { .. }
            | NativeValue::StaticDouble { .. }
            | NativeValue::StaticIntList { .. }
            | NativeValue::StaticRecord { .. }
            | NativeValue::StaticMap { .. }
            | NativeValue::StaticSet { .. }
            | NativeValue::StaticLambda { .. }
            | NativeValue::Null
            | NativeValue::Unit => {
                let actual = self.compile_expr(&actual_arguments[0])?;
                let Some(equal) = self.static_values_equal_user(expected, actual) else {
                    return Err(unsupported(
                        span,
                        "native assertResult for values with different types",
                    ));
                };
                if !equal {
                    self.emit_assert_result_failed_static_native(span, expected, actual);
                }
                Ok(NativeValue::Unit)
            }
            NativeValue::StaticList { label } => {
                let actual = self.compile_expr(&actual_arguments[0])?;
                if let NativeValue::RuntimeLinesList { data, len } = actual {
                    self.emit_runtime_lines_equal_static_list(
                        NativeStringRef {
                            data,
                            len: NativeStringLen::Runtime(len),
                        },
                        label,
                        span,
                    )?;
                    let ok = self.asm.create_text_label();
                    self.asm.cmp_reg_imm8(Reg::Rax, 0);
                    self.asm.jcc_label(Condition::NotEqual, ok);
                    self.emit_runtime_error(span, "assertResult failed");
                    self.asm.bind_text_label(ok);
                    return Ok(NativeValue::Unit);
                }
                let Some(equal) =
                    self.static_values_equal_user(NativeValue::StaticList { label }, actual)
                else {
                    return Err(unsupported(
                        span,
                        "native assertResult for values with different types",
                    ));
                };
                if !equal {
                    self.emit_assert_result_failed_static_native(
                        span,
                        NativeValue::StaticList { label },
                        actual,
                    );
                }
                Ok(NativeValue::Unit)
            }
            function @ NativeValue::BuiltinFunction { .. } => {
                let actual = self.compile_expr(&actual_arguments[0])?;
                let Some(equal) = self.static_values_equal_user(function, actual) else {
                    return Err(unsupported(
                        span,
                        "native assertResult for values with different types",
                    ));
                };
                if !equal {
                    self.emit_assert_result_failed_static_native(span, function, actual);
                }
                Ok(NativeValue::Unit)
            }
            NativeValue::RuntimeLinesList { data, len } => {
                let actual = self.compile_expr(&actual_arguments[0])?;
                match actual {
                    NativeValue::StaticList { label } => self
                        .emit_runtime_lines_equal_static_list(
                            NativeStringRef {
                                data,
                                len: NativeStringLen::Runtime(len),
                            },
                            label,
                            span,
                        )?,
                    NativeValue::RuntimeLinesList {
                        data: actual_data,
                        len: actual_len,
                    } => self.emit_runtime_lines_equal_runtime_lines(
                        NativeStringRef {
                            data,
                            len: NativeStringLen::Runtime(len),
                        },
                        NativeStringRef {
                            data: actual_data,
                            len: NativeStringLen::Runtime(actual_len),
                        },
                    ),
                    _ => {
                        return Err(unsupported(
                            span,
                            "native assertResult for runtime lines list and this value type",
                        ));
                    }
                };
                let ok = self.asm.create_text_label();
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.jcc_label(Condition::NotEqual, ok);
                self.emit_runtime_error(span, "assertResult failed");
                self.asm.bind_text_label(ok);
                Ok(NativeValue::Unit)
            }
        }
    }

    fn compile_inline_function_call(
        &mut self,
        function: &NativeFunction,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != function.params.len() {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "inline function expects {} arguments but got {}",
                    function.params.len(),
                    arguments.len()
                ),
            ));
        }
        self.push_scope();
        for (index, ((param, expected_value), argument)) in function
            .params
            .iter()
            .zip(function.param_values.iter().copied())
            .zip(arguments)
            .enumerate()
        {
            let flexible = function
                .flexible_params
                .get(index)
                .copied()
                .unwrap_or(false);
            let static_argument = self.static_value_from_pure_expr(argument);
            let value = self.compile_expr(argument)?;
            if !flexible && value != expected_value {
                self.pop_scope();
                return Err(unsupported(
                    span,
                    "native inline function argument for this value type",
                ));
            }
            match value {
                NativeValue::Int | NativeValue::Bool => {
                    let slot = self.allocate_slot(param.clone(), value);
                    self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                    if let Some(value) = static_argument {
                        self.bind_static_value(param.clone(), value);
                    }
                }
                NativeValue::Null
                | NativeValue::Unit
                | NativeValue::StaticFloat { .. }
                | NativeValue::StaticDouble { .. }
                | NativeValue::StaticString { .. }
                | NativeValue::RuntimeString { .. }
                | NativeValue::RuntimeLinesList { .. }
                | NativeValue::StaticIntList { .. }
                | NativeValue::StaticList { .. }
                | NativeValue::StaticRecord { .. }
                | NativeValue::StaticMap { .. }
                | NativeValue::StaticSet { .. }
                | NativeValue::StaticLambda { .. }
                | NativeValue::BuiltinFunction { .. } => {
                    self.bind_constant(param.clone(), value);
                }
            }
        }
        let value = self.compile_expr(&function.body)?;
        if self.native_value_captures_current_scope(value)
            || self.queued_threads_capture_current_scope()
        {
            self.pop_scope_preserving_allocations();
        } else {
            self.pop_scope();
        }
        if !function.flexible_return && value != function.return_value {
            return Err(unsupported(
                span,
                "native inline function return value with this type",
            ));
        }
        Ok(value)
    }

    fn compile_inline_lambda_call(
        &mut self,
        params: &[String],
        body: &Expr,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != params.len() {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "lambda expects {} arguments but got {}",
                    params.len(),
                    arguments.len()
                ),
            ));
        }
        self.push_scope();
        let result = (|| {
            for (param, argument) in params.iter().zip(arguments) {
                let static_argument = self.static_value_from_pure_expr(argument);
                let value = self.compile_expr(argument)?;
                match value {
                    NativeValue::Int | NativeValue::Bool => {
                        let slot = self.allocate_slot(param.clone(), value);
                        self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                        if let Some(value) = static_argument {
                            self.bind_static_value(param.clone(), value);
                        }
                    }
                    NativeValue::Null
                    | NativeValue::Unit
                    | NativeValue::StaticFloat { .. }
                    | NativeValue::StaticDouble { .. }
                    | NativeValue::StaticString { .. }
                    | NativeValue::RuntimeString { .. }
                    | NativeValue::RuntimeLinesList { .. }
                    | NativeValue::StaticIntList { .. }
                    | NativeValue::StaticList { .. }
                    | NativeValue::StaticRecord { .. }
                    | NativeValue::StaticMap { .. }
                    | NativeValue::StaticSet { .. }
                    | NativeValue::StaticLambda { .. }
                    | NativeValue::BuiltinFunction { .. } => {
                        self.bind_constant(param.clone(), value);
                    }
                }
            }
            self.compile_expr(body)
        })();
        match result {
            Ok(value) => {
                if self.native_value_captures_current_scope(value)
                    || self.queued_threads_capture_current_scope()
                {
                    self.pop_scope_preserving_allocations();
                } else {
                    self.pop_scope();
                }
                Ok(value)
            }
            Err(error) => {
                self.pop_scope();
                Err(error)
            }
        }
    }

    fn compile_cons(
        &mut self,
        head_arguments: &[Expr],
        tail_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if head_arguments.len() != 1 || tail_arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                "cons expects one head value and one tail list",
            ));
        }
        if self.expr_may_yield_runtime_lines_list(&tail_arguments[0]) {
            let head = self.compile_expr(&head_arguments[0])?;
            let Some(head) = self.native_string_ref(head) else {
                return Err(unsupported(
                    span,
                    "native cons for runtime lines list with non-string head",
                ));
            };
            let tail = self.compile_expr(&tail_arguments[0])?;
            let NativeValue::RuntimeLinesList { data, len } = tail else {
                return Err(unsupported(
                    span,
                    "native cons for runtime lines list with non-runtime tail",
                ));
            };
            return Ok(self.emit_runtime_lines_cons(
                head,
                NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                },
                span,
            ));
        }
        let head = self.static_value_from_argument_preserving_effects(
            &head_arguments[0],
            span,
            "native cons for non-static head",
        )?;
        let tail = self.static_value_from_argument_preserving_effects(
            &tail_arguments[0],
            span,
            "native cons for non-static tail",
        )?;
        let value = self
            .static_cons_value(head, tail)
            .ok_or_else(|| unsupported(span, "native cons for non-static list"))?;
        Ok(self.emit_static_value(&value))
    }

    fn compile_static_map(
        &mut self,
        list_arguments: &[Expr],
        mapper_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if list_arguments.len() != 1 || mapper_arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                "map expects one list and one mapper function",
            ));
        }
        let list = self.compile_expr(&list_arguments[0])?;
        let mapper = &mapper_arguments[0];
        match list {
            NativeValue::StaticIntList { label, len } => {
                let elements = self.asm.i64s_for_label(label, len);
                if let Expr::Lambda { params, body, .. } = mapper
                    && let [param] = params.as_slice()
                    && let Some(mapped) = elements
                        .iter()
                        .copied()
                        .map(|value| eval_const_int_expr_with_binding(body, param, value))
                        .collect::<Option<Vec<_>>>()
                {
                    let label = self.asm.data_label_with_i64s(&mapped);
                    return Ok(NativeValue::StaticIntList {
                        label,
                        len: mapped.len(),
                    });
                }
                let mut mapped = Vec::with_capacity(elements.len());
                for value in elements {
                    mapped.push(
                        self.compile_callable_with_static_arguments_preserving_effects(
                            mapper,
                            vec![StaticValue::Int(value)],
                            span,
                            "native map for this mapper body",
                        )?,
                    );
                }
                let value = self.static_list_value_from_elements(mapped);
                Ok(self.emit_static_value(&value))
            }
            NativeValue::StaticList { label } => {
                let elements = self
                    .static_lists
                    .get(label.0)
                    .map(|list| list.elements.clone())
                    .unwrap_or_default();
                let mut mapped = Vec::with_capacity(elements.len());
                for value in elements {
                    mapped.push(
                        self.compile_callable_with_static_arguments_preserving_effects(
                            mapper,
                            vec![value],
                            span,
                            "native map for this mapper body",
                        )?,
                    );
                }
                let label = self.intern_static_list(mapped);
                Ok(NativeValue::StaticList { label })
            }
            NativeValue::RuntimeLinesList { data, len } => self.compile_runtime_lines_map(
                NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                },
                mapper,
                span,
            ),
            _ => Err(unsupported(span, "native map for non-static list")),
        }
    }

    fn compile_runtime_lines_map(
        &mut self,
        input: NativeStringRef,
        mapper: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let mapper = self.compile_runtime_line_lambda(
            mapper,
            1,
            span,
            "native map over runtime lines for non-lambda mapper",
            "native map over runtime lines for this mapper arity",
        )?;
        if mapper.contains_thread_call {
            return Err(unsupported(
                span,
                "native map over runtime lines with thread mapper",
            ));
        }
        let [param] = mapper.params.as_slice() else {
            return Err(unsupported(
                span,
                "native map over runtime lines for this mapper arity",
            ));
        };

        const RUNTIME_STRING_CAP: usize = 65_536;
        let output = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let output_len = self.asm.data_label_with_i64s(&[0]);
        let output_offset = self.asm.data_label_with_i64s(&[0]);
        let line_data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let line_len = self.asm.data_label_with_i64s(&[0]);
        let cursor = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rax, output_offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::Rax, cursor);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);

        let saved_static_scopes = self.static_scopes.clone();
        self.static_scopes = vec![HashMap::new(); saved_static_scopes.len()];
        let assigned_names = assigned_names_in_expr(&mapper.body);

        let result = (|| {
            let loop_label = self.asm.create_text_label();
            let done = self.asm.create_text_label();
            let scan = self.asm.create_text_label();
            let segment_end = self.asm.create_text_label();
            let copy_loop = self.asm.create_text_label();
            let copied = self.asm.create_text_label();
            let consumed_at_end = self.asm.create_text_label();
            let body_label = self.asm.create_text_label();

            self.asm.bind_text_label(loop_label);
            self.asm.mov_data_addr(Reg::Rsi, input.data);
            self.emit_load_native_string_len(Reg::Rdx, input.len);
            self.asm.mov_data_addr(Reg::Rax, cursor);
            self.asm.load_ptr_disp32(Reg::R9, Reg::Rax, 0);
            self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
            self.asm.jcc_label(Condition::GreaterEqual, done);
            self.asm.mov_reg_reg(Reg::R8, Reg::R9);

            self.asm.bind_text_label(scan);
            self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
            self.asm.jcc_label(Condition::Equal, segment_end);
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
            self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
            self.asm.jcc_label(Condition::Equal, segment_end);
            self.asm.inc_reg(Reg::R8);
            self.asm.jmp_label(scan);

            self.asm.bind_text_label(segment_end);
            self.asm.mov_imm64(Reg::R10, 0);
            self.asm.bind_text_label(copy_loop);
            self.asm.cmp_reg_reg(Reg::R9, Reg::R8);
            self.asm.jcc_label(Condition::Equal, copied);
            self.emit_runtime_buffer_capacity_check(
                Reg::R10,
                RUNTIME_STRING_CAP,
                span,
                "map runtime line exceeds 65536 bytes",
            );
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R9);
            self.asm.mov_data_addr(Reg::Rbx, line_data);
            self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
            self.asm.inc_reg(Reg::R9);
            self.asm.inc_reg(Reg::R10);
            self.asm.jmp_label(copy_loop);

            self.asm.bind_text_label(copied);
            self.asm.mov_data_addr(Reg::Rax, line_len);
            self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R10);
            self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
            self.asm.jcc_label(Condition::Equal, consumed_at_end);
            self.asm.inc_reg(Reg::R8);
            self.asm.jmp_label(body_label);

            self.asm.bind_text_label(consumed_at_end);
            self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);

            self.asm.bind_text_label(body_label);
            self.asm.mov_data_addr(Reg::Rax, cursor);
            self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);

            self.dynamic_control_depth += 1;
            self.push_scope();
            self.bind_runtime_line_lambda_captures(&mapper);
            self.bind_constant(
                param.clone(),
                NativeValue::RuntimeString {
                    data: line_data,
                    len: line_len,
                },
            );
            let mapped = self.compile_expr(&mapper.body);
            self.pop_scope();
            self.dynamic_control_depth -= 1;
            let mapped = mapped?;

            let Some(mapped) = self.native_string_ref(mapped) else {
                return Err(unsupported(
                    span,
                    "native map over runtime lines for non-string mapper result",
                ));
            };
            self.emit_append_newline_separator_to_runtime_buffer_offset_label(
                output,
                output_offset,
                span,
                "map result exceeds 65536 bytes",
            );
            self.emit_append_native_string_to_runtime_buffer_offset_label(
                output,
                output_offset,
                mapped,
                span,
                "map result exceeds 65536 bytes",
            );
            self.asm.jmp_label(loop_label);

            self.asm.bind_text_label(done);
            self.asm.mov_data_addr(Reg::R10, output_offset);
            self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
            self.asm.mov_data_addr(Reg::R10, output_len);
            self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
            Ok(NativeValue::RuntimeLinesList {
                data: output,
                len: output_len,
            })
        })();

        self.static_scopes = saved_static_scopes;
        for name in assigned_names {
            self.remove_static_value(&name);
        }
        result
    }

    fn compile_runtime_line_lambda(
        &mut self,
        expr: &Expr,
        arity: usize,
        span: Span,
        non_lambda_feature: &str,
        arity_feature: &str,
    ) -> Result<StaticLambda, Diagnostic> {
        let lambda = match expr {
            Expr::Lambda { params, body, .. } => {
                let thread_aliases = self.current_thread_aliases();
                StaticLambda {
                    params: params.clone(),
                    body: body.as_ref().clone(),
                    captures: self.current_static_captures(),
                    runtime_captures: self.current_runtime_captures(),
                    contains_thread_call: expr_contains_thread_call(body, &thread_aliases),
                }
            }
            _ => {
                let value = self.compile_expr(expr)?;
                let NativeValue::StaticLambda { label } = value else {
                    return Err(unsupported(span, non_lambda_feature));
                };
                self.static_lambdas
                    .get(label.0)
                    .cloned()
                    .ok_or_else(|| unsupported(span, non_lambda_feature))?
            }
        };
        if lambda.params.len() != arity {
            return Err(unsupported(span, arity_feature));
        }
        Ok(lambda)
    }

    fn bind_runtime_line_lambda_captures(&mut self, lambda: &StaticLambda) {
        let assigned_captures = assigned_names_in_expr(&lambda.body);
        self.bind_static_lambda_runtime_captures(lambda);
        for (name, value) in lambda.captures.clone() {
            if self.static_capture_shadowed_by_runtime(lambda, &name)
                || (assigned_captures.contains(&name) && self.lookup_var(&name).is_some())
            {
                continue;
            }
            self.bind_static_runtime_value(name, value);
        }
    }

    fn compile_static_fold_left(
        &mut self,
        list_arguments: &[Expr],
        initial_arguments: &[Expr],
        reducer_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if list_arguments.len() != 1 || initial_arguments.len() != 1 || reducer_arguments.len() != 1
        {
            return Err(Diagnostic::compile(
                span,
                "foldLeft expects one list, one initial value, and one reducer function",
            ));
        }
        let list = self.compile_expr(&list_arguments[0])?;
        let reducer = &reducer_arguments[0];
        match list {
            NativeValue::StaticIntList { label, len } => {
                if let Expr::Lambda { params, body, .. } = reducer
                    && let [acc_name, element_name] = params.as_slice()
                    && let Some(mut acc) = const_int_expr(&initial_arguments[0])
                {
                    let mut folded = Some(acc);
                    for element in self.asm.i64s_for_label(label, len) {
                        folded = eval_const_int_expr_with_bindings(
                            body,
                            &[(acc_name.as_str(), acc), (element_name.as_str(), element)],
                        );
                        let Some(next) = folded else {
                            break;
                        };
                        acc = next;
                    }
                    if let Some(acc) = folded {
                        self.asm.mov_imm64(Reg::Rax, acc as u64);
                        return Ok(NativeValue::Int);
                    }
                }
                let mut acc = self.static_value_from_argument_preserving_effects(
                    &initial_arguments[0],
                    span,
                    "native foldLeft for non-static initial value",
                )?;
                for element in self.asm.i64s_for_label(label, len) {
                    acc = self.compile_callable_with_static_arguments_preserving_effects(
                        reducer,
                        vec![acc, StaticValue::Int(element)],
                        span,
                        "native foldLeft for this reducer body",
                    )?;
                }
                Ok(self.emit_static_value(&acc))
            }
            NativeValue::StaticList { label } => {
                let mut acc = self.static_value_from_argument_preserving_effects(
                    &initial_arguments[0],
                    span,
                    "native foldLeft for non-static initial value",
                )?;
                let elements = self
                    .static_lists
                    .get(label.0)
                    .map(|list| list.elements.clone())
                    .unwrap_or_default();
                for element in elements {
                    acc = self.compile_callable_with_static_arguments_preserving_effects(
                        reducer,
                        vec![acc, element],
                        span,
                        "native foldLeft for this reducer body",
                    )?;
                }
                Ok(self.emit_static_value(&acc))
            }
            NativeValue::RuntimeLinesList { data, len } => self.compile_runtime_lines_fold_left(
                NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                },
                &initial_arguments[0],
                reducer,
                span,
            ),
            _ => Err(unsupported(span, "native foldLeft for non-static list")),
        }
    }

    fn compile_runtime_lines_fold_left(
        &mut self,
        input: NativeStringRef,
        initial: &Expr,
        reducer: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let reducer = self.compile_runtime_line_lambda(
            reducer,
            2,
            span,
            "native foldLeft over runtime lines for non-lambda reducer",
            "native foldLeft over runtime lines for this reducer arity",
        )?;
        if reducer.contains_thread_call {
            return Err(unsupported(
                span,
                "native foldLeft over runtime lines with thread reducer",
            ));
        }
        let [acc_param, line_param] = reducer.params.as_slice() else {
            return Err(unsupported(
                span,
                "native foldLeft over runtime lines for this reducer arity",
            ));
        };

        #[derive(Clone, Copy)]
        enum RuntimeLineFoldAccumulator {
            Scalar {
                value: DataLabel,
                value_kind: NativeValue,
            },
            String {
                data: DataLabel,
                len: DataLabel,
            },
        }

        const RUNTIME_STRING_CAP: usize = 65_536;
        let line_data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let line_len = self.asm.data_label_with_i64s(&[0]);
        let cursor = self.asm.data_label_with_i64s(&[0]);

        let initial = self.compile_expr(initial)?;
        let accumulator = if matches!(initial, NativeValue::Int | NativeValue::Bool) {
            let value = self.asm.data_label_with_i64s(&[0]);
            self.asm.mov_data_addr(Reg::R10, value);
            self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rax);
            RuntimeLineFoldAccumulator::Scalar {
                value,
                value_kind: initial,
            }
        } else {
            let Some(initial) = self.native_string_ref(initial) else {
                return Err(unsupported(
                    span,
                    "native foldLeft over runtime lines for non-string, non-Int, or non-Bool initial value",
                ));
            };
            let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
            let len = self.asm.data_label_with_i64s(&[0]);
            self.emit_copy_native_string_to_runtime_string_buffer(
                data,
                len,
                initial,
                span,
                "foldLeft accumulator exceeds 65536 bytes",
            );
            RuntimeLineFoldAccumulator::String { data, len }
        };

        self.asm.mov_data_addr(Reg::Rax, cursor);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);

        let saved_static_scopes = self.static_scopes.clone();
        self.static_scopes = vec![HashMap::new(); saved_static_scopes.len()];
        let assigned_names = assigned_names_in_expr(&reducer.body);

        let result = (|| {
            let loop_label = self.asm.create_text_label();
            let done = self.asm.create_text_label();
            let scan = self.asm.create_text_label();
            let segment_end = self.asm.create_text_label();
            let copy_loop = self.asm.create_text_label();
            let copied = self.asm.create_text_label();
            let consumed_at_end = self.asm.create_text_label();
            let body_label = self.asm.create_text_label();

            self.asm.bind_text_label(loop_label);
            self.asm.mov_data_addr(Reg::Rsi, input.data);
            self.emit_load_native_string_len(Reg::Rdx, input.len);
            self.asm.mov_data_addr(Reg::Rax, cursor);
            self.asm.load_ptr_disp32(Reg::R9, Reg::Rax, 0);
            self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
            self.asm.jcc_label(Condition::GreaterEqual, done);
            self.asm.mov_reg_reg(Reg::R8, Reg::R9);

            self.asm.bind_text_label(scan);
            self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
            self.asm.jcc_label(Condition::Equal, segment_end);
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
            self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
            self.asm.jcc_label(Condition::Equal, segment_end);
            self.asm.inc_reg(Reg::R8);
            self.asm.jmp_label(scan);

            self.asm.bind_text_label(segment_end);
            self.asm.mov_imm64(Reg::R10, 0);
            self.asm.bind_text_label(copy_loop);
            self.asm.cmp_reg_reg(Reg::R9, Reg::R8);
            self.asm.jcc_label(Condition::Equal, copied);
            self.emit_runtime_buffer_capacity_check(
                Reg::R10,
                RUNTIME_STRING_CAP,
                span,
                "foldLeft runtime line exceeds 65536 bytes",
            );
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R9);
            self.asm.mov_data_addr(Reg::Rbx, line_data);
            self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
            self.asm.inc_reg(Reg::R9);
            self.asm.inc_reg(Reg::R10);
            self.asm.jmp_label(copy_loop);

            self.asm.bind_text_label(copied);
            self.asm.mov_data_addr(Reg::Rax, line_len);
            self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R10);
            self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
            self.asm.jcc_label(Condition::Equal, consumed_at_end);
            self.asm.inc_reg(Reg::R8);
            self.asm.jmp_label(body_label);

            self.asm.bind_text_label(consumed_at_end);
            self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);

            self.asm.bind_text_label(body_label);
            self.asm.mov_data_addr(Reg::Rax, cursor);
            self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);

            self.dynamic_control_depth += 1;
            self.push_scope();
            self.bind_runtime_line_lambda_captures(&reducer);
            match accumulator {
                RuntimeLineFoldAccumulator::Scalar { value, value_kind } => {
                    self.asm.mov_data_addr(Reg::Rax, value);
                    self.asm.load_ptr_disp32(Reg::Rax, Reg::Rax, 0);
                    let slot = self.allocate_slot(acc_param.clone(), value_kind);
                    self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                }
                RuntimeLineFoldAccumulator::String { data, len } => {
                    self.bind_constant(acc_param.clone(), NativeValue::RuntimeString { data, len });
                }
            }
            self.bind_constant(
                line_param.clone(),
                NativeValue::RuntimeString {
                    data: line_data,
                    len: line_len,
                },
            );
            let next_acc = self.compile_expr(&reducer.body);
            self.pop_scope();
            self.dynamic_control_depth -= 1;
            let next_acc = next_acc?;
            match accumulator {
                RuntimeLineFoldAccumulator::Scalar { value, value_kind } => {
                    if next_acc != value_kind {
                        return Err(unsupported(
                            span,
                            "native foldLeft over runtime lines for reducer result with different scalar type",
                        ));
                    }
                    self.asm.mov_data_addr(Reg::R10, value);
                    self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rax);
                }
                RuntimeLineFoldAccumulator::String { data, len } => {
                    let Some(next_acc) = self.native_string_ref(next_acc) else {
                        return Err(unsupported(
                            span,
                            "native foldLeft over runtime lines for non-string reducer result",
                        ));
                    };
                    self.emit_copy_native_string_to_runtime_string_buffer(
                        data,
                        len,
                        next_acc,
                        span,
                        "foldLeft accumulator exceeds 65536 bytes",
                    );
                }
            }
            self.asm.jmp_label(loop_label);

            self.asm.bind_text_label(done);
            match accumulator {
                RuntimeLineFoldAccumulator::Scalar { value, value_kind } => {
                    self.asm.mov_data_addr(Reg::Rax, value);
                    self.asm.load_ptr_disp32(Reg::Rax, Reg::Rax, 0);
                    Ok(value_kind)
                }
                RuntimeLineFoldAccumulator::String { data, len } => {
                    Ok(NativeValue::RuntimeString { data, len })
                }
            }
        })();

        self.static_scopes = saved_static_scopes;
        for name in assigned_names {
            self.remove_static_value(&name);
        }
        result
    }

    fn compile_static_head(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                format!("head expects 1 argument but got {}", arguments.len()),
            ));
        }
        let value = self.compile_expr(&arguments[0])?;
        match value {
            NativeValue::StaticIntList { label, len } => {
                if len == 0 {
                    self.emit_head_empty(span);
                    return Ok(NativeValue::Unit);
                }
                self.asm.mov_data_addr(Reg::Rax, label);
                self.asm.load_ptr_disp32(Reg::Rax, Reg::Rax, 0);
                Ok(NativeValue::Int)
            }
            NativeValue::StaticList { label } => {
                let Some(value) = self
                    .static_lists
                    .get(label.0)
                    .and_then(|list| list.elements.first())
                    .cloned()
                else {
                    self.emit_head_empty(span);
                    return Ok(NativeValue::Unit);
                };
                Ok(self.emit_static_value(&value))
            }
            NativeValue::RuntimeLinesList { data, len } => {
                let input = NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                };
                Ok(self.emit_runtime_lines_head(input, span))
            }
            _ => Err(unsupported(span, "native head for non-static list")),
        }
    }

    fn compile_static_curried_fold_like_call(
        &mut self,
        callee: &Expr,
        arguments: &[Expr],
        span: Span,
    ) -> Result<Option<NativeValue>, Diagnostic> {
        let Some((name, groups)) = flatten_curried_call(callee, arguments) else {
            return Ok(None);
        };
        if !self.functions.contains_key(name)
            || groups.len() != 3
            || groups.iter().any(|group| group.len() != 1)
            || !matches!(groups[2][0], Expr::Lambda { .. })
        {
            return Ok(None);
        }
        self.compile_static_fold_left(groups[0], groups[1], groups[2], span)
            .map(Some)
    }

    fn compile_static_string_helper(
        &mut self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        match name {
            "toString" => {
                self.expect_static_arity(name, arguments, 1, span)?;
                if self.expr_may_yield_runtime_string(&arguments[0]) {
                    let input = self.compile_expr(&arguments[0])?;
                    if self.native_string_ref(input).is_some() {
                        return Ok(input);
                    }
                    return Err(unsupported(
                        span,
                        "native toString for non-string runtime value",
                    ));
                }
                if let Some(value) = self.static_value_from_pure_expr(&arguments[0]) {
                    return Ok(self.emit_static_string(self.static_value_display_string(&value)));
                }
                let before_static_scopes = self.static_scopes.clone();
                let preview = self.preview_static_value_after_effectful_eval(&arguments[0]);
                self.static_scopes = before_static_scopes;
                if preview.is_some() {
                    let value = self.static_value_from_argument_preserving_effects(
                        &arguments[0],
                        span,
                        "native toString for non-static value",
                    )?;
                    return Ok(self.emit_static_string(self.static_value_display_string(&value)));
                }
                let value = self.compile_expr(&arguments[0])?;
                self.emit_runtime_to_string_value(value, span)
            }
            "substring" => {
                self.expect_static_arity(name, arguments, 3, span)?;
                if self.expr_may_yield_runtime_string(&arguments[0]) {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, "native substring for non-string"));
                    };
                    if matches!(
                        (
                            self.preview_static_value_after_effectful_eval(&arguments[1]),
                            self.preview_static_value_after_effectful_eval(&arguments[2])
                        ),
                        (Some(StaticValue::Int(_)), Some(StaticValue::Int(_)))
                    ) {
                        let start = self.static_non_negative_int_argument_preserving_effects(
                            &arguments[1],
                            name,
                            span,
                        )?;
                        let end = self.static_non_negative_int_argument_preserving_effects(
                            &arguments[2],
                            name,
                            span,
                        )?;
                        return Ok(self.emit_runtime_string_slice(
                            input,
                            start,
                            end.max(start),
                            span,
                        ));
                    }
                    let start = self.compile_expr(&arguments[1])?;
                    if start != NativeValue::Int {
                        return Err(unsupported(
                            span,
                            "native substring for non-integer start index",
                        ));
                    }
                    let start_chars = self.asm.data_label_with_i64s(&[0]);
                    self.asm.mov_data_addr(Reg::Rcx, start_chars);
                    self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::Rax);
                    let end = self.compile_expr(&arguments[2])?;
                    if end != NativeValue::Int {
                        return Err(unsupported(
                            span,
                            "native substring for non-integer end index",
                        ));
                    }
                    let end_chars = self.asm.data_label_with_i64s(&[0]);
                    self.asm.mov_data_addr(Reg::Rcx, end_chars);
                    self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::Rax);
                    return Ok(self.emit_runtime_string_slice_dynamic_indices(
                        input,
                        start_chars,
                        end_chars,
                        span,
                        name,
                    ));
                }
                let input =
                    self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
                let start = self.static_non_negative_int_argument_preserving_effects(
                    &arguments[1],
                    name,
                    span,
                )?;
                let end = self.static_non_negative_int_argument_preserving_effects(
                    &arguments[2],
                    name,
                    span,
                )?;
                let chars = input.chars().collect::<Vec<_>>();
                let start = start.min(chars.len());
                let end = end.min(chars.len()).max(start);
                Ok(self.emit_static_string(chars[start..end].iter().collect()))
            }
            "at" => {
                self.expect_static_arity(name, arguments, 2, span)?;
                if self.expr_may_yield_runtime_string(&arguments[0]) {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, "native at for non-string"));
                    };
                    if matches!(
                        self.preview_static_value_after_effectful_eval(&arguments[1]),
                        Some(StaticValue::Int(_))
                    ) {
                        let index = self.static_non_negative_int_argument_preserving_effects(
                            &arguments[1],
                            name,
                            span,
                        )?;
                        return Ok(self.emit_runtime_string_slice(input, index, index + 1, span));
                    }
                    let index = self.compile_expr(&arguments[1])?;
                    if index != NativeValue::Int {
                        return Err(unsupported(span, "native at for non-integer index"));
                    }
                    let start_chars = self.asm.data_label_with_i64s(&[0]);
                    self.asm.mov_data_addr(Reg::Rcx, start_chars);
                    self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::Rax);
                    let end_chars = self.asm.data_label_with_i64s(&[0]);
                    self.asm.inc_reg(Reg::Rax);
                    self.asm.mov_data_addr(Reg::Rcx, end_chars);
                    self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::Rax);
                    return Ok(self.emit_runtime_string_slice_dynamic_indices(
                        input,
                        start_chars,
                        end_chars,
                        span,
                        name,
                    ));
                }
                let input =
                    self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
                let index = self.static_non_negative_int_argument_preserving_effects(
                    &arguments[1],
                    name,
                    span,
                )?;
                let chars = input.chars().collect::<Vec<_>>();
                let value = chars
                    .get(index.min(chars.len()))
                    .copied()
                    .unwrap_or_default();
                Ok(self.emit_static_string(if value == '\0' {
                    String::new()
                } else {
                    value.to_string()
                }))
            }
            "matches" => {
                self.expect_static_arity(name, arguments, 2, span)?;
                if self.expr_may_yield_runtime_string(&arguments[0])
                    || self.expr_may_yield_runtime_string(&arguments[1])
                {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, "native matches for non-string"));
                    };
                    let pattern = self.compile_expr(&arguments[1])?;
                    let Some(pattern) = self.native_string_ref(pattern) else {
                        return Err(unsupported(span, "native matches for non-string pattern"));
                    };
                    self.emit_runtime_string_matches_pattern(input, pattern);
                    return Ok(NativeValue::Bool);
                }
                let input =
                    self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
                let pattern =
                    self.static_string_from_argument_preserving_effects(&arguments[1], span, name)?;
                self.asm
                    .mov_imm64(Reg::Rax, u64::from(simple_regex_is_match(&input, &pattern)));
                Ok(NativeValue::Bool)
            }
            "split" => {
                self.expect_static_arity(name, arguments, 2, span)?;
                if self.expr_may_yield_runtime_string(&arguments[0]) {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, "native split for non-string"));
                    };
                    if self.expr_may_yield_runtime_string(&arguments[1]) {
                        let delimiter = self.compile_expr(&arguments[1])?;
                        let Some(delimiter) = self.native_string_ref(delimiter) else {
                            return Err(unsupported(span, "native split for non-string delimiter"));
                        };
                        return Ok(self
                            .emit_runtime_string_split_runtime_delimiter(input, delimiter, span));
                    }
                    let delimiter = self.static_string_from_argument_preserving_effects(
                        &arguments[1],
                        span,
                        name,
                    )?;
                    return self.emit_runtime_string_split_static_delimiter(input, delimiter, span);
                }
                let input =
                    self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
                let delimiter =
                    self.static_string_from_argument_preserving_effects(&arguments[1], span, name)?;
                let elements = if delimiter.is_empty() {
                    input
                        .chars()
                        .map(|ch| self.static_string_value(ch.to_string()))
                        .collect::<Vec<_>>()
                } else {
                    input
                        .split(&delimiter)
                        .map(|part| self.static_string_value(part.to_string()))
                        .collect::<Vec<_>>()
                };
                let label = self.intern_static_list(elements);
                Ok(NativeValue::StaticList { label })
            }
            "trim" | "trimLeft" | "trimRight" | "toLowerCase" | "toUpperCase" | "reverse" => {
                self.expect_static_arity(name, arguments, 1, span)?;
                if matches!(name, "trim" | "trimLeft" | "trimRight")
                    && self.expr_may_yield_runtime_string(&arguments[0])
                {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, &format!("native {name} for non-string")));
                    };
                    return Ok(self.emit_runtime_string_trim(
                        input,
                        matches!(name, "trim" | "trimLeft"),
                        matches!(name, "trim" | "trimRight"),
                        span,
                    ));
                }
                if matches!(name, "toLowerCase" | "toUpperCase")
                    && self.expr_may_yield_runtime_string(&arguments[0])
                {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, &format!("native {name} for non-string")));
                    };
                    return Ok(self.emit_runtime_string_ascii_case(input, name == "toUpperCase"));
                }
                if name == "reverse" && self.expr_may_yield_runtime_string(&arguments[0]) {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, "native reverse for non-string"));
                    };
                    return Ok(self.emit_runtime_string_reverse(input));
                }
                let input =
                    self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
                let output = match name {
                    "trim" => input.trim().to_string(),
                    "trimLeft" => input.trim_start().to_string(),
                    "trimRight" => input.trim_end().to_string(),
                    "toLowerCase" => input.to_lowercase(),
                    "toUpperCase" => input.to_uppercase(),
                    "reverse" => input.chars().rev().collect(),
                    _ => unreachable!("string unary helper matched above"),
                };
                Ok(self.emit_static_string(output))
            }
            "replace" | "replaceAll" => {
                self.expect_static_arity(name, arguments, 3, span)?;
                if self.expr_may_yield_runtime_string(&arguments[0]) {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, "native replace for non-string"));
                    };
                    if name == "replace" {
                        let from = self.compile_expr(&arguments[1])?;
                        let Some(from) = self.native_string_ref(from) else {
                            return Err(unsupported(span, "native replace for non-string pattern"));
                        };
                        let to = self.compile_expr(&arguments[2])?;
                        let Some(to) = self.native_string_ref(to) else {
                            return Err(unsupported(
                                span,
                                "native replace for non-string replacement",
                            ));
                        };
                        return Ok(
                            self.emit_runtime_string_replace_first_dynamic(input, from, to, span)
                        );
                    }
                    let from = self.static_string_from_argument_preserving_effects(
                        &arguments[1],
                        span,
                        name,
                    )?;
                    let to = self.static_string_from_argument_preserving_effects(
                        &arguments[2],
                        span,
                        name,
                    )?;
                    return Ok(
                        self.emit_runtime_string_replace_all_static_pattern(input, from, to, span)
                    );
                }
                let input =
                    self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
                let from =
                    self.static_string_from_argument_preserving_effects(&arguments[1], span, name)?;
                let to =
                    self.static_string_from_argument_preserving_effects(&arguments[2], span, name)?;
                let output = if name == "replace" {
                    input.replacen(&from, &to, 1)
                } else {
                    simple_regex_replace_all(&input, &from, &to)
                };
                Ok(self.emit_static_string(output))
            }
            "startsWith" | "endsWith" | "contains" => {
                self.compile_string_predicate_helper(name, arguments, span)
            }
            "isEmptyString" => {
                self.expect_static_arity(name, arguments, 1, span)?;
                let input = self.compile_expr(&arguments[0])?;
                match input {
                    NativeValue::StaticString { len, .. } => {
                        self.asm.mov_imm64(Reg::Rax, u64::from(len == 0));
                    }
                    NativeValue::RuntimeString { len, .. } => {
                        self.emit_load_native_string_len(Reg::Rax, NativeStringLen::Runtime(len));
                        self.asm.cmp_reg_imm8(Reg::Rax, 0);
                        self.asm.setcc_al(Condition::Equal);
                        self.asm.movzx_rax_al();
                    }
                    _ => return Err(unsupported(span, "native isEmptyString for non-string")),
                }
                Ok(NativeValue::Bool)
            }
            "indexOf" | "lastIndexOf" => {
                self.expect_static_arity(name, arguments, 2, span)?;
                let input = self.compile_expr(&arguments[0])?;
                let Some(input) = self.native_string_ref(input) else {
                    return Err(unsupported(span, &format!("native {name} for non-string")));
                };
                let needle = self.compile_expr(&arguments[1])?;
                let Some(needle) = self.native_string_ref(needle) else {
                    return Err(unsupported(span, &format!("native {name} for non-string")));
                };
                self.emit_native_string_index_of(input, needle, name == "lastIndexOf");
                Ok(NativeValue::Int)
            }
            "length" => {
                self.expect_static_arity(name, arguments, 1, span)?;
                let input = self.compile_expr(&arguments[0])?;
                match input {
                    NativeValue::StaticString { label, len } => {
                        let input = self.string_from_data_label(label, len, span, name)?;
                        self.asm.mov_imm64(Reg::Rax, input.chars().count() as u64);
                    }
                    value => {
                        let Some(input) = self.native_string_ref(value) else {
                            return Err(unsupported(span, "native length for non-string"));
                        };
                        self.emit_runtime_string_char_length(input);
                    }
                }
                Ok(NativeValue::Int)
            }
            "repeat" => {
                self.expect_static_arity(name, arguments, 2, span)?;
                if self.expr_may_yield_runtime_string(&arguments[0]) {
                    let input = self.compile_expr(&arguments[0])?;
                    let Some(input) = self.native_string_ref(input) else {
                        return Err(unsupported(span, "native repeat for non-string"));
                    };
                    if matches!(
                        self.preview_static_value_after_effectful_eval(&arguments[1]),
                        Some(StaticValue::Int(_))
                    ) {
                        let count = self.static_non_negative_int_argument_preserving_effects(
                            &arguments[1],
                            name,
                            span,
                        )?;
                        return Ok(self.emit_runtime_string_repeat(input, count, span));
                    }
                    let count = self.compile_expr(&arguments[1])?;
                    if count != NativeValue::Int {
                        return Err(unsupported(span, "native repeat for non-integer count"));
                    }
                    return Ok(self.emit_runtime_string_repeat_dynamic_count(input, span));
                }
                let input =
                    self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
                let count = self.static_non_negative_int_argument_preserving_effects(
                    &arguments[1],
                    name,
                    span,
                )?;
                Ok(self.emit_static_string(input.repeat(count)))
            }
            _ => Err(unsupported(span, "native string helper")),
        }
    }

    fn compile_static_join(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("join", arguments, 2, span)?;
        let list = self.compile_expr(&arguments[0])?;
        if let NativeValue::RuntimeLinesList { data, len } = list {
            if self.expr_may_yield_runtime_string(&arguments[1]) {
                let delimiter = self.compile_expr(&arguments[1])?;
                let Some(delimiter) = self.native_string_ref(delimiter) else {
                    return Err(unsupported(span, "native join for non-string delimiter"));
                };
                return Ok(self.emit_runtime_lines_join_runtime_delimiter(
                    NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    },
                    delimiter,
                    span,
                ));
            }
            let delimiter =
                self.static_string_from_argument_preserving_effects(&arguments[1], span, "join")?;
            let delimiter_label = self.asm.data_label_with_bytes(delimiter.as_bytes());
            return Ok(self.emit_runtime_lines_join(
                NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                },
                delimiter_label,
                delimiter.len(),
                span,
            ));
        }
        let delimiter =
            self.static_string_from_argument_preserving_effects(&arguments[1], span, "join")?;
        let NativeValue::StaticList { label } = list else {
            return Err(unsupported(span, "native join for non-static string list"));
        };
        let elements = self
            .static_lists
            .get(label.0)
            .map(|list| list.elements.clone())
            .unwrap_or_default();
        let parts = elements
            .iter()
            .map(|value| match value {
                StaticValue::StaticString { label, len } => {
                    self.string_from_data_label(*label, *len, span, "join")
                }
                _ => Err(unsupported(span, "native join for non-string list element")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.emit_static_string(parts.join(&delimiter)))
    }

    fn compile_numeric_helper(
        &mut self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                format!("{name} expects 1 argument but got {}", arguments.len()),
            ));
        }
        if let Some(value) = self.static_numeric_call_value(name, arguments) {
            return Ok(self.emit_static_value(&value));
        }
        if let Some(argument_value) = self.preview_static_value_after_effectful_eval(&arguments[0])
            && self
                .static_numeric_call_value_from_value(name, argument_value)
                .is_some()
        {
            let argument_value = self.static_value_from_argument_preserving_effects(
                &arguments[0],
                span,
                "native numeric helper for non-static value",
            )?;
            if let Some(value) = self.static_numeric_call_value_from_value(name, argument_value) {
                return Ok(self.emit_static_value(&value));
            }
        }
        match name {
            "int" | "floor" | "ceil" => {
                let value = self.compile_expr(&arguments[0])?;
                if value != NativeValue::Int {
                    return Err(unsupported(
                        span,
                        "native integer numeric helper for non-Int",
                    ));
                }
                Ok(NativeValue::Int)
            }
            "abs" => {
                let value = self.compile_expr(&arguments[0])?;
                if value != NativeValue::Int {
                    return Err(unsupported(span, "native abs for non-Int"));
                }
                let done = self.asm.create_text_label();
                self.asm.cmp_reg_imm8(Reg::Rax, 0);
                self.asm.jcc_label(Condition::GreaterEqual, done);
                self.asm.neg_reg(Reg::Rax);
                self.asm.bind_text_label(done);
                Ok(NativeValue::Int)
            }
            _ => Err(unsupported(
                span,
                "native numeric helper for non-static value",
            )),
        }
    }

    fn compile_sleep(&mut self, arguments: &[Expr], span: Span) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                format!("sleep expects 1 argument but got {}", arguments.len()),
            ));
        }
        if let Some(millis) = const_int_expr(&arguments[0]) {
            if millis < 0 {
                self.emit_runtime_error(span, "sleep expects a non-negative integer index");
                return Ok(NativeValue::Unit);
            }
            let seconds = millis / 1000;
            let nanos = (millis % 1000) * 1_000_000;
            let timespec = self.asm.data_label_with_i64s(&[seconds, nanos]);
            self.asm.mov_imm64(Reg::Rax, 35);
            self.asm.mov_data_addr(Reg::Rdi, timespec);
            self.asm.mov_imm64(Reg::Rsi, 0);
            self.asm.syscall();
            return Ok(NativeValue::Unit);
        }

        let value = self.compile_expr(&arguments[0])?;
        if value != NativeValue::Int {
            return Err(unsupported(
                span,
                "native sleep for non-Int millisecond argument",
            ));
        }
        let ok = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::GreaterEqual, ok);
        self.emit_runtime_error(span, "sleep expects a non-negative integer index");
        self.asm.bind_text_label(ok);

        self.asm.mov_imm64(Reg::Rbx, 1000);
        self.asm.cqo();
        self.asm.idiv_reg(Reg::Rbx);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::Rdx);
        self.asm.mov_imm64(Reg::Rbx, 1_000_000);
        self.asm.imul_reg_reg(Reg::Rcx, Reg::Rbx);
        self.asm.push_reg(Reg::Rcx);
        self.asm.push_reg(Reg::Rax);
        self.asm.mov_imm64(Reg::Rax, 35);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::Rsp);
        self.asm.mov_imm64(Reg::Rsi, 0);
        self.asm.syscall();
        self.asm.add_reg_imm32(Reg::Rsp, 16);
        Ok(NativeValue::Unit)
    }

    fn compile_thread(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                format!("thread expects 1 argument but got {}", arguments.len()),
            ));
        }
        let Expr::Lambda { params, body, .. } = &arguments[0] else {
            return Err(unsupported(span, "native thread for non-lambda argument"));
        };
        if !params.is_empty() {
            return Err(unsupported(
                span,
                "native thread for lambda with parameters",
            ));
        }
        let queued_thread = QueuedThread {
            body: body.as_ref().clone(),
            captures: self.current_static_captures(),
            runtime_captures: self.current_runtime_captures(),
        };
        if self.dynamic_control_depth > 0
            && self.mergeable_dynamic_branch_depth == 0
            && self.queued_thread_captures_current_scope(&queued_thread)
        {
            return Err(unsupported(
                span,
                "native thread capturing dynamic branch locals",
            ));
        }
        self.queued_threads.push(queued_thread);
        Ok(NativeValue::Unit)
    }

    fn emit_queued_threads(&mut self) -> Result<(), Diagnostic> {
        for thread in std::mem::take(&mut self.queued_threads) {
            self.push_scope();
            self.bind_queued_thread_captures(&thread);
            self.compile_expr(&thread.body)?;
            self.pop_scope();
        }
        Ok(())
    }

    fn compile_stopwatch(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                format!("stopwatch expects 1 argument but got {}", arguments.len()),
            ));
        }
        let Expr::Lambda { params, body, .. } = &arguments[0] else {
            return Err(unsupported(
                span,
                "native stopwatch for non-lambda argument",
            ));
        };
        if !params.is_empty() {
            return Err(unsupported(
                span,
                "native stopwatch for lambda with parameters",
            ));
        }

        let start = self.asm.data_label_with_i64s(&[0, 0]);
        let end = self.asm.data_label_with_i64s(&[0, 0]);
        self.emit_clock_gettime(start);
        self.push_scope();
        self.compile_expr(body)?;
        if self.queued_threads_capture_current_scope() {
            self.pop_scope_preserving_allocations();
        } else {
            self.pop_scope();
        }
        self.emit_clock_gettime(end);
        self.emit_elapsed_millis(start, end);
        Ok(NativeValue::Int)
    }

    fn emit_clock_gettime(&mut self, output: DataLabel) {
        self.asm.mov_imm64(Reg::Rax, 228);
        self.asm.mov_imm64(Reg::Rdi, 1);
        self.asm.mov_data_addr(Reg::Rsi, output);
        self.asm.syscall();
    }

    fn emit_elapsed_millis(&mut self, start: DataLabel, end: DataLabel) {
        self.asm.mov_data_addr(Reg::Rax, end);
        self.asm.load_ptr_disp32(Reg::Rax, Reg::Rax, 0);
        self.asm.mov_data_addr(Reg::Rcx, start);
        self.asm.load_ptr_disp32(Reg::Rcx, Reg::Rcx, 0);
        self.asm.sub_reg_reg(Reg::Rax, Reg::Rcx);
        self.asm.mov_imm64(Reg::Rcx, 1000);
        self.asm.imul_reg_reg(Reg::Rax, Reg::Rcx);
        self.asm.push_reg(Reg::Rax);

        self.asm.mov_data_addr(Reg::Rax, end);
        self.asm.load_ptr_disp32(Reg::Rax, Reg::Rax, 8);
        self.asm.mov_data_addr(Reg::Rcx, start);
        self.asm.load_ptr_disp32(Reg::Rcx, Reg::Rcx, 8);
        self.asm.sub_reg_reg(Reg::Rax, Reg::Rcx);
        self.asm.cqo();
        self.asm.mov_imm64(Reg::Rbx, 1_000_000);
        self.asm.idiv_reg(Reg::Rbx);
        self.asm.pop_reg(Reg::Rcx);
        self.asm.add_reg_reg(Reg::Rax, Reg::Rcx);
    }

    fn compile_static_method_call(
        &mut self,
        target: &Expr,
        field: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let target_static = self.static_value_from_pure_expr(target);
        let record_candidate = target_static
            .clone()
            .or_else(|| self.static_result_after_effectful_eval(target, &[]));
        if let Some(StaticValue::StaticRecord {
            label: candidate_label,
        }) = record_candidate.clone()
            && matches!(
                self.static_record_field(candidate_label, field),
                Some(StaticValue::StaticLambda { .. })
            )
        {
            let receiver = if let Some(receiver) = target_static {
                receiver
            } else {
                self.static_value_from_argument_preserving_effects(
                    target,
                    span,
                    "native static lambda method receiver",
                )?
            };
            let StaticValue::StaticRecord {
                label: record_label,
            } = receiver
            else {
                return Err(unsupported(span, "native static lambda method receiver"));
            };
            let Some(StaticValue::StaticLambda { label }) =
                self.static_record_field(record_label, field)
            else {
                return Err(unsupported(span, "native static lambda method"));
            };
            return self.compile_static_lambda_method_call(receiver, label, arguments, span);
        }
        if let Some(StaticValue::StaticRecord {
            label: candidate_label,
        }) = record_candidate
            && matches!(
                self.static_record_field(candidate_label, field),
                Some(StaticValue::BuiltinFunction { .. })
            )
        {
            let receiver = if let Some(receiver) = target_static {
                receiver
            } else {
                self.static_value_from_argument_preserving_effects(
                    target,
                    span,
                    "native static builtin method receiver",
                )?
            };
            let StaticValue::StaticRecord {
                label: record_label,
            } = receiver
            else {
                return Err(unsupported(span, "native static builtin method receiver"));
            };
            let Some(StaticValue::BuiltinFunction { label }) =
                self.static_record_field(record_label, field)
            else {
                return Err(unsupported(span, "native static builtin method"));
            };
            let name = self
                .builtin_aliases
                .get(label.0)
                .cloned()
                .ok_or_else(|| unsupported(span, "native static builtin method"))?;
            let callee = Expr::Identifier { name, span };
            return self.compile_call(&callee, arguments, span);
        }
        let helper_name = match field {
            "map" => return self.compile_static_map(std::slice::from_ref(target), arguments, span),
            "bind" => {
                if arguments.len() != 1 {
                    return Err(Diagnostic::compile(
                        span,
                        format!("bind expects 1 argument but got {}", arguments.len()),
                    ));
                }
                let mapper_arguments = self
                    .static_list_values_from_argument_preserving_effects(
                        target,
                        span,
                        "native bind for non-static list",
                    )?
                    .into_iter()
                    .take(1)
                    .collect();
                let value = self.compile_callable_with_static_arguments_preserving_effects(
                    &arguments[0],
                    mapper_arguments,
                    span,
                    "native bind for this mapper body",
                )?;
                return Ok(self.emit_static_value(&value));
            }
            "toString" => "toString",
            "substring" => "substring",
            "at" => "at",
            "matches" => "matches",
            "split" => "split",
            "join" => "join",
            "trim" => "trim",
            "trimLeft" => "trimLeft",
            "trimRight" => "trimRight",
            "replace" => "replace",
            "replaceAll" => "replaceAll",
            "toLowerCase" => "toLowerCase",
            "toUpperCase" => "toUpperCase",
            "startsWith" => "startsWith",
            "endsWith" => "endsWith",
            "contains" => "contains",
            "containsKey" => "containsKey",
            "containsValue" => "containsValue",
            "get" => "get",
            "head" => "head",
            "tail" => "tail",
            "size" => "size",
            "isEmpty"
                if matches!(target_static, Some(StaticValue::StaticString { .. }))
                    || self.expr_may_yield_runtime_string(target) =>
            {
                "isEmptyString"
            }
            "isEmpty" => "isEmpty",
            "isEmptyString" => "isEmptyString",
            "indexOf" => "indexOf",
            "lastIndexOf" => "lastIndexOf",
            "length" => "length",
            "repeat" => "repeat",
            "reverse" => "reverse",
            _ => return Err(unsupported(span, "native method call")),
        };
        let mut lowered = Vec::with_capacity(arguments.len() + 1);
        lowered.push(target.clone());
        lowered.extend(arguments.iter().cloned());
        let callee = Expr::Identifier {
            name: helper_name.to_string(),
            span,
        };
        self.compile_call(&callee, &lowered, span)
    }

    fn compile_static_lambda_method_call(
        &mut self,
        receiver: StaticValue,
        label: LambdaLabel,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let lambda = self
            .static_lambdas
            .get(label.0)
            .cloned()
            .ok_or_else(|| unsupported(span, "native static lambda method"))?;
        let receives_receiver = if lambda.params.len() == arguments.len() + 1 {
            true
        } else if lambda.params.len() == arguments.len() {
            false
        } else {
            return Err(unsupported(
                span,
                "native static lambda method with this arity",
            ));
        };

        if static_expr_is_pure(&lambda.body)
            && !self.lambda_uses_runtime_captures(&lambda)
            && arguments.iter().all(|argument| {
                self.preview_static_value_after_effectful_eval(argument)
                    .is_some()
            })
        {
            let values = self.static_values_from_arguments_preserving_effects(
                arguments,
                span,
                "native static lambda method argument",
            )?;
            let mut bindings = Vec::with_capacity(lambda.params.len());
            if receives_receiver {
                bindings.push((lambda.params[0].as_str(), receiver.clone()));
            }
            let params = lambda.params.iter().skip(usize::from(receives_receiver));
            for (param, value) in params.zip(values) {
                bindings.push((param.as_str(), value));
            }
            self.push_scope();
            self.bind_static_lambda_captures(&lambda);
            if let Some(value) = self.static_value_from_expr_with_bindings(&lambda.body, &bindings)
            {
                self.pop_scope();
                return Ok(self.emit_static_value(&value));
            }
            self.pop_scope();
        }

        self.push_scope();
        let value = (|| {
            let assigned_captures = assigned_names_in_expr(&lambda.body);
            self.bind_static_lambda_runtime_captures(&lambda);
            for (name, value) in lambda.captures.clone() {
                if self.static_capture_shadowed_by_runtime(&lambda, &name)
                    || (assigned_captures.contains(&name) && self.lookup_var(&name).is_some())
                {
                    continue;
                }
                self.bind_static_runtime_value(name, value);
            }
            let mut params = lambda.params.iter();
            if receives_receiver {
                let receiver_param = params
                    .next()
                    .expect("receiver arity was checked before binding");
                self.bind_static_runtime_value(receiver_param.clone(), receiver);
            }
            for (param, argument) in params.zip(arguments) {
                let static_argument = self.static_value_from_pure_expr(argument);
                let value = self.compile_expr(argument)?;
                match value {
                    NativeValue::Int | NativeValue::Bool => {
                        let slot = self.allocate_slot(param.clone(), value);
                        self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                        if let Some(value) = static_argument {
                            self.bind_static_value(param.clone(), value);
                        }
                    }
                    NativeValue::Null
                    | NativeValue::Unit
                    | NativeValue::StaticFloat { .. }
                    | NativeValue::StaticDouble { .. }
                    | NativeValue::StaticString { .. }
                    | NativeValue::RuntimeString { .. }
                    | NativeValue::RuntimeLinesList { .. }
                    | NativeValue::StaticIntList { .. }
                    | NativeValue::StaticList { .. }
                    | NativeValue::StaticRecord { .. }
                    | NativeValue::StaticMap { .. }
                    | NativeValue::StaticSet { .. }
                    | NativeValue::StaticLambda { .. }
                    | NativeValue::BuiltinFunction { .. } => {
                        self.bind_constant(param.clone(), value);
                    }
                }
            }
            self.compile_expr(&lambda.body)
        })();
        match value {
            Ok(value) => {
                if self.native_value_captures_current_scope(value)
                    || self.queued_threads_capture_current_scope()
                {
                    self.pop_scope_preserving_allocations();
                } else {
                    self.pop_scope();
                }
                Ok(value)
            }
            Err(error) => {
                self.pop_scope();
                Err(error)
            }
        }
    }

    fn compile_static_contains_direct(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 2 {
            return Err(Diagnostic::compile(
                span,
                format!("contains expects 2 arguments but got {}", arguments.len()),
            ));
        }
        let value = self.compile_expr(&arguments[0])?;
        match value {
            NativeValue::StaticString { .. } | NativeValue::RuntimeString { .. } => {
                let input = self
                    .native_string_ref(value)
                    .expect("string value should expose native string ref");
                let needle = self.compile_expr(&arguments[1])?;
                let Some(needle) = self.native_string_ref(needle) else {
                    return Err(unsupported(span, "native contains for non-static string"));
                };
                self.emit_native_string_contains(input, needle);
                Ok(NativeValue::Bool)
            }
            NativeValue::StaticSet { label } => {
                let elements = self
                    .static_sets
                    .get(label.0)
                    .map(|set| set.elements.clone())
                    .unwrap_or_default();
                self.compile_static_values_contains(
                    elements,
                    &arguments[1],
                    span,
                    "native Set#contains for non-static value",
                )
            }
            NativeValue::StaticIntList { .. } | NativeValue::StaticList { .. } => {
                let collection = self
                    .static_value_from_native(value)
                    .expect("static list value should convert back to StaticValue");
                let elements = self
                    .static_list_values_from_value(&collection)
                    .expect("static list should expose elements");
                self.compile_static_values_contains(
                    elements,
                    &arguments[1],
                    span,
                    "native contains for non-static value",
                )
            }
            NativeValue::RuntimeLinesList { data, len } => {
                let needle = self.compile_expr(&arguments[1])?;
                let Some(needle) = self.native_string_ref(needle) else {
                    return Err(unsupported(
                        span,
                        "native runtime lines contains for non-string needle",
                    ));
                };
                self.emit_runtime_lines_contains_string(
                    NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    },
                    needle,
                );
                Ok(NativeValue::Bool)
            }
            _ => Err(unsupported(
                span,
                "native contains for non-static string, list, or set",
            )),
        }
    }

    fn compile_static_values_contains(
        &mut self,
        elements: Vec<StaticValue>,
        needle: &Expr,
        span: Span,
        unsupported_message: &'static str,
    ) -> Result<NativeValue, Diagnostic> {
        let before_static_scopes = self.static_scopes.clone();
        let needle_preview = self.preview_static_value_after_effectful_eval(needle);
        self.static_scopes = before_static_scopes;
        if needle_preview.is_none() {
            let needle = self.compile_expr(needle)?;
            if let Some(needle) = self.native_string_ref(needle) {
                let candidates = elements
                    .iter()
                    .filter_map(|element| self.static_value_string_ref(element))
                    .collect::<Vec<_>>();
                self.emit_static_string_membership(needle, candidates);
                return Ok(NativeValue::Bool);
            }
            if matches!(needle, NativeValue::Int | NativeValue::Bool) {
                let candidates = elements
                    .iter()
                    .filter_map(|element| Self::static_value_scalar_bits(element, needle))
                    .collect::<Vec<_>>();
                self.emit_static_scalar_membership(candidates);
                return Ok(NativeValue::Bool);
            }
            return Err(unsupported(span, unsupported_message));
        }
        let needle =
            self.static_value_from_argument_preserving_effects(needle, span, unsupported_message)?;
        let contains = elements
            .iter()
            .any(|element| self.static_value_equal_user(element, &needle));
        self.asm.mov_imm64(Reg::Rax, u64::from(contains));
        Ok(NativeValue::Bool)
    }

    fn static_value_scalar_bits(value: &StaticValue, kind: NativeValue) -> Option<u64> {
        match (kind, value) {
            (NativeValue::Int, StaticValue::Int(value)) => Some(*value as u64),
            (NativeValue::Bool, StaticValue::Bool(value)) => Some(u64::from(*value)),
            _ => None,
        }
    }

    fn emit_static_scalar_membership(&mut self, candidates: impl IntoIterator<Item = u64>) {
        let found = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.mov_reg_reg(Reg::R10, Reg::Rax);
        for candidate in candidates {
            self.asm.mov_imm64(Reg::Rax, candidate);
            self.asm.cmp_reg_reg(Reg::R10, Reg::Rax);
            self.asm.jcc_label(Condition::Equal, found);
        }
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(found);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.bind_text_label(done);
    }

    fn compile_static_map_contains_key_direct(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 2 {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "Map#containsKey expects 2 arguments but got {}",
                    arguments.len()
                ),
            ));
        }
        self.compile_static_map_contains_key(
            std::slice::from_ref(&arguments[0]),
            std::slice::from_ref(&arguments[1]),
            span,
        )
    }

    fn compile_static_map_contains_key(
        &mut self,
        map_arguments: &[Expr],
        key_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if map_arguments.len() != 1 || key_arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                "Map#containsKey expects one map and one key",
            ));
        }
        let entries = self.static_map_entries_from_expr(&map_arguments[0], span)?;
        let keys = entries
            .into_iter()
            .map(|(entry_key, _)| entry_key)
            .collect();
        self.compile_static_values_contains(
            keys,
            &key_arguments[0],
            span,
            "native Map#containsKey for non-static key",
        )
    }

    fn compile_static_map_contains_value_direct(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 2 {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "Map#containsValue expects 2 arguments but got {}",
                    arguments.len()
                ),
            ));
        }
        self.compile_static_map_contains_value(
            std::slice::from_ref(&arguments[0]),
            std::slice::from_ref(&arguments[1]),
            span,
        )
    }

    fn compile_static_map_contains_value(
        &mut self,
        map_arguments: &[Expr],
        value_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if map_arguments.len() != 1 || value_arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                "Map#containsValue expects one map and one value",
            ));
        }
        let entries = self.static_map_entries_from_expr(&map_arguments[0], span)?;
        let values = entries
            .into_iter()
            .map(|(_, entry_value)| entry_value)
            .collect();
        self.compile_static_values_contains(
            values,
            &value_arguments[0],
            span,
            "native Map#containsValue for non-static value",
        )
    }

    fn compile_static_map_get_direct(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 2 {
            return Err(Diagnostic::compile(
                span,
                format!("Map#get expects 2 arguments but got {}", arguments.len()),
            ));
        }
        self.compile_static_map_get(
            std::slice::from_ref(&arguments[0]),
            std::slice::from_ref(&arguments[1]),
            span,
        )
    }

    fn compile_static_map_get(
        &mut self,
        map_arguments: &[Expr],
        key_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if map_arguments.len() != 1 || key_arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                "Map#get expects one map and one key",
            ));
        }
        let entries = self.static_map_entries_from_expr(&map_arguments[0], span)?;
        let key = self.static_value_from_argument_preserving_effects(
            &key_arguments[0],
            span,
            "native Map#get for non-static key",
        )?;
        let value = entries
            .iter()
            .find(|(entry_key, _)| self.static_value_equal_user(entry_key, &key))
            .map(|(_, value)| value.clone())
            .unwrap_or(StaticValue::Null);
        Ok(self.emit_static_value(&value))
    }

    fn compile_static_set_contains_direct(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if arguments.len() != 2 {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "Set#contains expects 2 arguments but got {}",
                    arguments.len()
                ),
            ));
        }
        self.compile_static_set_contains(
            std::slice::from_ref(&arguments[0]),
            std::slice::from_ref(&arguments[1]),
            span,
        )
    }

    fn compile_static_set_contains(
        &mut self,
        set_arguments: &[Expr],
        value_arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if set_arguments.len() != 1 || value_arguments.len() != 1 {
            return Err(Diagnostic::compile(
                span,
                "Set#contains expects one set and one value",
            ));
        }
        let collection = self.compile_expr(&set_arguments[0])?;
        match collection {
            NativeValue::RuntimeLinesList { data, len } => {
                let needle = self.compile_expr(&value_arguments[0])?;
                let Some(needle) = self.native_string_ref(needle) else {
                    return Err(unsupported(
                        span,
                        "native runtime lines contains for non-string needle",
                    ));
                };
                self.emit_runtime_lines_contains_string(
                    NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    },
                    needle,
                );
                return Ok(NativeValue::Bool);
            }
            NativeValue::StaticString { .. } | NativeValue::RuntimeString { .. } => {
                let input = self
                    .native_string_ref(collection)
                    .expect("string value should expose native string ref");
                let needle = self.compile_expr(&value_arguments[0])?;
                let Some(needle) = self.native_string_ref(needle) else {
                    return Err(unsupported(span, "native contains for non-static string"));
                };
                self.emit_native_string_contains(input, needle);
                return Ok(NativeValue::Bool);
            }
            NativeValue::StaticSet { label } => {
                let elements = self
                    .static_sets
                    .get(label.0)
                    .map(|set| set.elements.clone())
                    .unwrap_or_default();
                return self.compile_static_values_contains(
                    elements,
                    &value_arguments[0],
                    span,
                    "native Set#contains for non-static value",
                );
            }
            NativeValue::StaticIntList { .. } | NativeValue::StaticList { .. } => {
                let collection = self
                    .static_value_from_native(collection)
                    .expect("static list value should convert back to StaticValue");
                let elements = self
                    .static_list_values_from_value(&collection)
                    .expect("static list should expose elements");
                return self.compile_static_values_contains(
                    elements,
                    &value_arguments[0],
                    span,
                    "native contains for non-static value",
                );
            }
            _ => {}
        }
        Err(unsupported(span, "native set helper for non-static set"))
    }

    fn static_value_string_ref(&self, value: &StaticValue) -> Option<NativeStringRef> {
        let StaticValue::StaticString { label, len } = value else {
            return None;
        };
        Some(NativeStringRef {
            data: *label,
            len: NativeStringLen::Immediate(*len),
        })
    }

    fn emit_static_string_membership(
        &mut self,
        needle: NativeStringRef,
        candidates: impl IntoIterator<Item = NativeStringRef>,
    ) {
        let found = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        for candidate in candidates {
            self.emit_native_string_equality(needle, candidate);
            self.asm.cmp_reg_imm8(Reg::Rax, 0);
            self.asm.jcc_label(Condition::NotEqual, found);
        }
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(found);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.bind_text_label(done);
    }

    fn static_map_entries_from_expr(
        &mut self,
        expr: &Expr,
        span: Span,
    ) -> Result<Vec<(StaticValue, StaticValue)>, Diagnostic> {
        let value = self.compile_expr(expr)?;
        let NativeValue::StaticMap { label } = value else {
            return Err(unsupported(span, "native map helper for non-static map"));
        };
        Ok(self
            .static_maps
            .get(label.0)
            .map(|map| map.entries.clone())
            .unwrap_or_default())
    }

    fn compile_file_output_write(
        &mut self,
        arguments: &[Expr],
        append: bool,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let name = if append {
            "FileOutput#append"
        } else {
            "FileOutput#write"
        };
        self.expect_static_arity(name, arguments, 2, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label = self.compile_runtime_path_argument(&arguments[0], span, name)?;
            if self.expr_may_yield_runtime_string(&arguments[1]) {
                let content = self.compile_expr(&arguments[1])?;
                let Some(content) = self.native_string_ref(content) else {
                    return Err(unsupported(
                        span,
                        &format!("native {name} for non-string content"),
                    ));
                };
                self.emit_file_write_runtime_string_to_path_label(
                    path_label, content, append, span, name,
                );
            } else {
                let content =
                    self.static_string_from_argument_preserving_effects(&arguments[1], span, name)?;
                self.emit_file_write_to_path_label(
                    path_label,
                    content.as_bytes(),
                    append,
                    span,
                    name,
                );
            }
            return Ok(NativeValue::Unit);
        }
        let path =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
        if self.expr_may_yield_runtime_string(&arguments[1]) {
            let content = self.compile_expr(&arguments[1])?;
            let Some(content) = self.native_string_ref(content) else {
                return Err(unsupported(
                    span,
                    &format!("native {name} for non-string content"),
                ));
            };
            self.emit_file_write_runtime_string(&path, content, append, span, name);
            self.virtual_files.remove(&path);
            self.unknown_virtual_paths.insert(path);
            return Ok(NativeValue::Unit);
        }
        let content =
            self.static_string_from_argument_preserving_effects(&arguments[1], span, name)?;
        self.emit_file_write(&path, content.as_bytes(), append, span, name);
        if append {
            if self.unknown_virtual_paths.contains(&path) {
                self.virtual_files.remove(&path);
            } else {
                self.virtual_files
                    .entry(path)
                    .and_modify(|existing| existing.push_str(&content))
                    .or_insert(content);
            }
        } else {
            self.unknown_virtual_paths.remove(&path);
            self.virtual_files.insert(path, content);
        }
        Ok(NativeValue::Unit)
    }

    fn compile_file_output_write_lines(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("FileOutput#writeLines", arguments, 2, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "FileOutput#writeLines")?;
            let content = self.compile_expr(&arguments[1])?;
            if let Some(content) = self.runtime_write_lines_content_ref(content, span)? {
                self.emit_file_write_runtime_string_to_path_label(
                    path_label,
                    content,
                    false,
                    span,
                    "FileOutput#writeLines",
                );
            } else {
                let content = self.static_write_lines_content_from_native(
                    content,
                    span,
                    "FileOutput#writeLines",
                )?;
                self.emit_file_write_to_path_label(
                    path_label,
                    content.as_bytes(),
                    false,
                    span,
                    "FileOutput#writeLines",
                );
            }
            return Ok(NativeValue::Unit);
        }
        let path = self.static_string_from_argument_preserving_effects(
            &arguments[0],
            span,
            "FileOutput#writeLines",
        )?;
        let content = self.compile_expr(&arguments[1])?;
        if let Some(content) = self.runtime_write_lines_content_ref(content, span)? {
            self.emit_file_write_runtime_string(
                &path,
                content,
                false,
                span,
                "FileOutput#writeLines",
            );
            self.virtual_files.remove(&path);
            self.unknown_virtual_paths.insert(path);
        } else {
            let content = self.static_write_lines_content_from_native(
                content,
                span,
                "FileOutput#writeLines",
            )?;
            self.emit_file_write(
                &path,
                content.as_bytes(),
                false,
                span,
                "FileOutput#writeLines",
            );
            self.unknown_virtual_paths.remove(&path);
            self.virtual_files.insert(path, content);
        }
        Ok(NativeValue::Unit)
    }

    fn compile_file_output_exists(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("FileOutput#exists", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "FileOutput#exists")?;
            self.emit_runtime_path_exists_label(path_label);
            return Ok(NativeValue::Bool);
        }
        let path = self.static_string_from_argument_preserving_effects(
            &arguments[0],
            span,
            "FileOutput#exists",
        )?;
        if self.unknown_virtual_paths.contains(&path) {
            self.emit_runtime_path_exists(&path);
        } else {
            let exists =
                self.virtual_files.contains_key(&path) || std::path::Path::new(&path).exists();
            self.asm.mov_imm64(Reg::Rax, u64::from(exists));
        }
        Ok(NativeValue::Bool)
    }

    fn compile_file_output_delete(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("FileOutput#delete", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "FileOutput#delete")?;
            self.emit_file_delete_label(path_label, span, "FileOutput#delete");
            return Ok(NativeValue::Unit);
        }
        let path = self.static_string_from_argument_preserving_effects(
            &arguments[0],
            span,
            "FileOutput#delete",
        )?;
        self.emit_file_delete(&path, span, "FileOutput#delete");
        self.unknown_virtual_paths.remove(&path);
        self.virtual_files.remove(&path);
        Ok(NativeValue::Unit)
    }

    fn compile_standard_input_all(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("StandardInput#all", arguments, 0, span)?;
        Ok(self.emit_standard_input_to_runtime_string(span, "StandardInput#all"))
    }

    fn compile_standard_input_lines(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("StandardInput#lines", arguments, 0, span)?;
        let NativeValue::RuntimeString { data, len } =
            self.emit_standard_input_to_runtime_string(span, "StandardInput#lines")
        else {
            return Err(unsupported(span, "native StandardInput#lines runtime list"));
        };
        Ok(NativeValue::RuntimeLinesList { data, len })
    }

    fn emit_standard_input_to_runtime_string(&mut self, span: Span, name: &str) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let overflow = self.asm.data_label_with_bytes(&[0]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_imm64(Reg::Rdi, 0);
        self.asm.mov_data_addr(Reg::Rsi, data);
        self.asm.mov_imm64(Reg::Rdx, RUNTIME_STRING_CAP as u64);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to read stdin"));
        self.asm.mov_data_addr(Reg::Rcx, len);
        self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::Rax);
        self.asm.mov_imm64(Reg::Rcx, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rcx);
        let done = self.asm.create_text_label();
        self.asm.jcc_label(Condition::NotEqual, done);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_imm64(Reg::Rdi, 0);
        self.asm.mov_data_addr(Reg::Rsi, overflow);
        self.asm.mov_imm64(Reg::Rdx, 1);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to read stdin"));
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_error(span, &format!("{name} runtime string exceeds 65536 bytes"));

        self.asm.bind_text_label(done);
        NativeValue::RuntimeString { data, len }
    }

    fn compile_environment_vars(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Environment#vars", arguments, 0, span)?;
        Ok(self.emit_environment_vars(span))
    }

    fn emit_environment_vars(&mut self, span: Span) -> NativeValue {
        const RUNTIME_LIST_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_LIST_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);
        let cursor = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.mov_data_addr(Reg::R10, self.environment_base);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R10, Reg::R10, 0);
        self.asm.load_ptr_disp32(Reg::Rsi, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::Rsi, 0);
        self.asm.jcc_label(Condition::Equal, done);

        self.emit_append_newline_separator_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "Environment#vars result exceeds 65536 bytes",
        );
        self.emit_append_c_string_pointer_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "Environment#vars result exceeds 65536 bytes",
        );

        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.add_reg_imm32(Reg::R8, 8);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.emit_store_runtime_string_len_from_offset(len, offset);
        NativeValue::RuntimeLinesList { data, len }
    }

    fn compile_environment_get(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Environment#get", arguments, 1, span)?;
        let key = self.compile_environment_key_argument(&arguments[0], span, "Environment#get")?;
        Ok(self.emit_environment_get_key_ref(
            key,
            span,
            "Environment#get missing environment variable",
        ))
    }

    fn compile_environment_exists(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Environment#exists", arguments, 1, span)?;
        let key =
            self.compile_environment_key_argument(&arguments[0], span, "Environment#exists")?;
        self.emit_environment_exists_key_ref(key);
        Ok(NativeValue::Bool)
    }

    fn compile_environment_key_argument(
        &mut self,
        argument: &Expr,
        span: Span,
        name: &str,
    ) -> Result<NativeStringRef, Diagnostic> {
        let value = self.compile_expr(argument)?;
        self.native_string_ref(value)
            .ok_or_else(|| unsupported(span, &format!("native {name} for non-string key")))
    }

    fn emit_environment_get_static_key(
        &mut self,
        key: &str,
        span: Span,
        missing_message: &str,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        let found = self.emit_find_environment_static_key(key);
        let not_found = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.jcc_label(Condition::Equal, not_found);
        self.asm.bind_text_label(found);
        self.asm.add_reg_imm32(Reg::Rsi, (key.len() + 1) as i32);
        self.emit_append_c_string_pointer_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "Environment#get result exceeds 65536 bytes",
        );
        self.emit_store_runtime_string_len_from_offset(len, offset);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_found);
        self.emit_runtime_error(span, missing_message);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_to_string_value(
        &mut self,
        value: NativeValue,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        if self.native_string_ref(value).is_some() {
            return Ok(value);
        }
        if let NativeValue::RuntimeLinesList { data, len } = value {
            return Ok(self.emit_runtime_lines_list_to_runtime_string(
                NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                },
                span,
            ));
        }
        let text = match value {
            NativeValue::Int => {
                self.emit_i64_rax_to_runtime_string_ref(span, "toString result exceeds 65536 bytes")
            }
            NativeValue::Bool => self
                .emit_bool_rax_to_runtime_string_ref(span, "toString result exceeds 65536 bytes"),
            _ => return Err(unsupported(span, "native toString for non-static value")),
        };
        let NativeStringLen::Runtime(len) = text.len else {
            return Err(unsupported(span, "native toString for non-static value"));
        };
        Ok(NativeValue::RuntimeString {
            data: text.data,
            len,
        })
    }

    fn emit_environment_get_key_ref(
        &mut self,
        key: NativeStringRef,
        span: Span,
        missing_message: &str,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        let found = self.emit_find_environment_key_ref(key);
        let not_found = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.jcc_label(Condition::Equal, not_found);
        self.asm.bind_text_label(found);
        self.emit_load_native_string_len(Reg::Rdx, key.len);
        self.asm.add_reg_reg(Reg::Rsi, Reg::Rdx);
        self.asm.add_reg_imm32(Reg::Rsi, 1);
        self.emit_append_c_string_pointer_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "Environment#get result exceeds 65536 bytes",
        );
        self.emit_store_runtime_string_len_from_offset(len, offset);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_found);
        self.emit_runtime_error(span, missing_message);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_environment_get_static_key_or_default(
        &mut self,
        key: &str,
        default: &str,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);
        let default_label = self.nul_terminated_data_label(default);

        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        let found = self.emit_find_environment_static_key(key);
        let use_default = self.asm.create_text_label();
        let done_copy = self.asm.create_text_label();
        self.asm.jcc_label(Condition::Equal, use_default);
        self.asm.bind_text_label(found);
        self.asm.add_reg_imm32(Reg::Rsi, (key.len() + 1) as i32);
        self.emit_append_c_string_pointer_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "environment value exceeds 65536 bytes",
        );
        self.asm.jmp_label(done_copy);
        self.asm.bind_text_label(use_default);
        self.asm.mov_data_addr(Reg::Rsi, default_label);
        self.emit_append_c_string_pointer_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "environment default value exceeds 65536 bytes",
        );
        self.asm.bind_text_label(done_copy);
        self.emit_store_runtime_string_len_from_offset(len, offset);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_environment_exists_key_ref(&mut self, key: NativeStringRef) {
        let found = self.emit_find_environment_key_ref(key);
        let done = self.asm.create_text_label();
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.bind_text_label(found);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.bind_text_label(done);
    }

    fn emit_find_environment_static_key(&mut self, key: &str) -> TextLabel {
        let mut prefix = key.as_bytes().to_vec();
        prefix.push(b'=');
        let prefix_len = prefix.len() as i32;
        let prefix_label = self.asm.data_label_with_bytes(&prefix);
        let cursor = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::R10, self.environment_base);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        let loop_label = self.asm.create_text_label();
        let compare_loop = self.asm.create_text_label();
        let next_entry = self.asm.create_text_label();
        let found = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R10, Reg::R10, 0);
        self.asm.load_ptr_disp32(Reg::Rsi, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::Rsi, 0);
        self.asm.jcc_label(Condition::Equal, done);

        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(compare_loop);
        self.asm.cmp_reg_imm32(Reg::R8, prefix_len);
        self.asm.jcc_label(Condition::Equal, found);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_data_addr(Reg::Rdx, prefix_label);
        self.asm.movzx_byte_indexed(Reg::Rbx, Reg::Rdx, Reg::R8);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, next_entry);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(compare_loop);

        self.asm.bind_text_label(next_entry);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.add_reg_imm32(Reg::R8, 8);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.asm.cmp_reg_imm8(Reg::Rsi, 0);
        found
    }

    fn emit_find_environment_key_ref(&mut self, key: NativeStringRef) -> TextLabel {
        let cursor = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::R10, self.environment_base);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        let loop_label = self.asm.create_text_label();
        let compare_loop = self.asm.create_text_label();
        let check_equals = self.asm.create_text_label();
        let next_entry = self.asm.create_text_label();
        let found = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R10, Reg::R10, 0);
        self.asm.load_ptr_disp32(Reg::Rsi, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::Rsi, 0);
        self.asm.jcc_label(Condition::Equal, done);

        self.emit_load_native_string_len(Reg::Rdx, key.len);
        self.asm.mov_data_addr(Reg::R10, key.data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(compare_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, check_equals);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rbx, Reg::R10, Reg::R8);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, next_entry);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(compare_loop);

        self.asm.bind_text_label(check_equals);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'=' as i8);
        self.asm.jcc_label(Condition::Equal, found);

        self.asm.bind_text_label(next_entry);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.add_reg_imm32(Reg::R8, 8);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.asm.cmp_reg_imm8(Reg::Rsi, 0);
        found
    }

    fn compile_command_line_args(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("CommandLine#args", arguments, 0, span)?;
        Ok(self.emit_command_line_args(span))
    }

    fn emit_command_line_args(&mut self, span: Span) -> NativeValue {
        const RUNTIME_LIST_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_LIST_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);
        let index = self.asm.data_label_with_i64s(&[0]);
        let cursor = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.mov_data_addr(Reg::R10, index);
        self.asm.mov_imm64(Reg::R8, 1);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm
            .mov_data_addr(Reg::R10, self.command_line_argv1_base);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.mov_data_addr(Reg::R10, index);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, self.command_line_argc);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, done);

        self.emit_append_newline_separator_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "CommandLine#args result exceeds 65536 bytes",
        );
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R10, Reg::R10, 0);
        self.asm.load_ptr_disp32(Reg::Rsi, Reg::R10, 0);
        self.emit_append_c_string_pointer_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            "CommandLine#args result exceeds 65536 bytes",
        );

        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.add_reg_imm32(Reg::R8, 8);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.mov_data_addr(Reg::R10, index);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.inc_reg(Reg::R8);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.emit_store_runtime_string_len_from_offset(len, offset);
        NativeValue::RuntimeLinesList { data, len }
    }

    fn compile_process_exit(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Process#exit", arguments, 1, span)?;
        if let Some(code) = const_int_expr(&arguments[0]) {
            if code < 0 {
                self.emit_runtime_error(span, "Process#exit expects a non-negative integer index");
                return Ok(NativeValue::Unit);
            }
            self.asm.mov_imm64(Reg::Rax, 60);
            self.asm.mov_imm64(Reg::Rdi, code as u64);
            self.asm.syscall();
            return Ok(NativeValue::Unit);
        }

        let value = self.compile_expr(&arguments[0])?;
        if value != NativeValue::Int {
            return Err(unsupported(span, "native Process#exit for non-Int code"));
        }
        let ok = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::GreaterEqual, ok);
        self.emit_runtime_error(span, "Process#exit expects a non-negative integer index");
        self.asm.bind_text_label(ok);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::Rax);
        self.asm.mov_imm64(Reg::Rax, 60);
        self.asm.syscall();
        Ok(NativeValue::Unit)
    }

    fn compile_file_input_open(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("FileInput#open", arguments, 2, span)?;
        let callback = match &arguments[1] {
            Expr::Lambda { params, body, .. } => Some((params.as_slice(), body.as_ref())),
            Expr::Block { expressions, .. } if expressions.len() == 1 => {
                if let Expr::Lambda { params, body, .. } = &expressions[0] {
                    Some((params.as_slice(), body.as_ref()))
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some((params, body)) = callback else {
            return Err(unsupported(
                span,
                "native FileInput#open for non-lambda callback",
            ));
        };
        let [_param] = params else {
            return Err(unsupported(
                span,
                "native FileInput#open for callback with this arity",
            ));
        };
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let Some(read_name) = self.file_input_open_callback_name(body, &params[0]) else {
                return Err(unsupported(
                    span,
                    "native FileInput#open runtime path callback",
                ));
            };
            if !matches!(read_name.as_str(), "FileInput#all" | "FileInput#readAll")
                && !matches!(
                    read_name.as_str(),
                    "FileInput#lines" | "FileInput#readLines"
                )
            {
                return Err(unsupported(
                    span,
                    "native FileInput#open runtime path callback",
                ));
            }
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "FileInput#open")?;
            let content =
                self.emit_file_read_to_runtime_string_from_path_label(path_label, span, &read_name);
            if matches!(
                read_name.as_str(),
                "FileInput#lines" | "FileInput#readLines"
            ) {
                let NativeValue::RuntimeString { data, len } = content else {
                    return Err(unsupported(span, "native FileInput#open runtime lines"));
                };
                return Ok(NativeValue::RuntimeLinesList { data, len });
            }
            return Ok(content);
        }
        let path = self.static_string_from_argument_preserving_effects(
            &arguments[0],
            span,
            "FileInput#open",
        )?;
        let path_value = self.static_string_value(path);
        let value = self.compile_lambda_body_with_static_arguments_preserving_effects(
            params,
            body,
            vec![path_value],
            span,
            "native FileInput#open callback body",
        )?;
        Ok(self.emit_static_value(&value))
    }

    fn file_input_open_callback_name(&self, body: &Expr, param: &str) -> Option<String> {
        let body = match body {
            Expr::Block { expressions, .. } if expressions.len() == 1 => &expressions[0],
            body => body,
        };
        let Expr::Call {
            callee, arguments, ..
        } = body
        else {
            return None;
        };
        let Expr::Identifier { name, .. } = callee.as_ref() else {
            return None;
        };
        if arguments.len() != 1
            || !matches!(&arguments[0], Expr::Identifier { name, .. } if name == param)
        {
            return None;
        }
        let name = self.builtin_name_for_identifier(name);
        matches!(
            name.as_str(),
            "FileInput#all" | "FileInput#readAll" | "FileInput#lines" | "FileInput#readLines"
        )
        .then_some(name)
    }

    fn compile_file_input_all(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("FileInput#all", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "FileInput#all")?;
            return Ok(self.emit_file_read_to_runtime_string_from_path_label(
                path_label,
                span,
                "FileInput#all",
            ));
        }
        let path = self.static_string_from_argument_preserving_effects(
            &arguments[0],
            span,
            "FileInput#all",
        )?;
        if let Some(content) = self.virtual_files.get(&path).cloned() {
            return Ok(self.emit_static_string(content));
        }
        if self.unknown_virtual_paths.contains(&path) {
            return Ok(self.emit_file_read_to_runtime_string(&path, span, "FileInput#all"));
        }
        match fs::read_to_string(&path) {
            Ok(content) => Ok(self.emit_static_string(content)),
            Err(_) => Ok(self.emit_file_read_to_runtime_string(&path, span, "FileInput#all")),
        }
    }

    fn compile_file_input_lines(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("FileInput#lines", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "FileInput#lines")?;
            let content = self.emit_file_read_to_runtime_string_from_path_label(
                path_label,
                span,
                "FileInput#lines",
            );
            let NativeValue::RuntimeString { data, len } = content else {
                return Err(unsupported(span, "native FileInput#lines runtime list"));
            };
            return Ok(NativeValue::RuntimeLinesList { data, len });
        }
        let path = self.static_string_from_argument_preserving_effects(
            &arguments[0],
            span,
            "FileInput#lines",
        )?;
        let content = self.static_file_content(&path, span, "FileInput#lines")?;
        let elements = content
            .lines()
            .map(|line| self.static_string_value(line.to_string()))
            .collect::<Vec<_>>();
        let label = self.intern_static_list(elements);
        Ok(NativeValue::StaticList { label })
    }

    fn static_file_content(
        &self,
        path: &str,
        span: Span,
        name: &str,
    ) -> Result<String, Diagnostic> {
        if let Some(content) = self.virtual_files.get(path) {
            return Ok(content.clone());
        }
        self.ensure_virtual_path_known(path, span, name)?;
        fs::read_to_string(path)
            .map_err(|error| Diagnostic::compile(span, format!("native {name} failed: {error}")))
    }

    fn emit_file_write(&mut self, path: &str, bytes: &[u8], append: bool, span: Span, name: &str) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_file_write_to_path_label(path_label, bytes, append, span, name);
    }

    fn emit_file_write_to_path_label(
        &mut self,
        path_label: DataLabel,
        bytes: &[u8],
        append: bool,
        span: Span,
        name: &str,
    ) {
        let content_label = self.asm.data_label_with_bytes(bytes);
        let flags = if append { 1 | 64 | 1024 } else { 1 | 64 | 512 };
        self.asm.mov_imm64(Reg::Rax, 2);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.mov_imm64(Reg::Rsi, flags);
        self.asm.mov_imm64(Reg::Rdx, 0o644);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to open file"));
        self.asm.push_reg(Reg::Rax);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::Rax);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.mov_data_addr(Reg::Rsi, content_label);
        self.asm.mov_imm64(Reg::Rdx, bytes.len() as u64);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to write file"));
        self.asm.pop_reg(Reg::Rdi);
        self.asm.mov_imm64(Reg::Rax, 3);
        self.asm.syscall();
    }

    fn emit_file_write_runtime_string(
        &mut self,
        path: &str,
        content: NativeStringRef,
        append: bool,
        span: Span,
        name: &str,
    ) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_file_write_runtime_string_to_path_label(path_label, content, append, span, name);
    }

    fn emit_file_write_runtime_string_to_path_label(
        &mut self,
        path_label: DataLabel,
        content: NativeStringRef,
        append: bool,
        span: Span,
        name: &str,
    ) {
        let flags = if append { 1 | 64 | 1024 } else { 1 | 64 | 512 };
        self.asm.mov_imm64(Reg::Rax, 2);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.mov_imm64(Reg::Rsi, flags);
        self.asm.mov_imm64(Reg::Rdx, 0o644);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to open file"));
        self.asm.push_reg(Reg::Rax);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::Rax);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.mov_data_addr(Reg::Rsi, content.data);
        self.emit_load_native_string_len(Reg::Rdx, content.len);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to write file"));
        self.asm.pop_reg(Reg::Rdi);
        self.asm.mov_imm64(Reg::Rax, 3);
        self.asm.syscall();
    }

    fn emit_file_delete(&mut self, path: &str, span: Span, name: &str) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_file_delete_label(path_label, span, name);
    }

    fn emit_file_delete_label(&mut self, path_label: DataLabel, span: Span, name: &str) {
        self.asm.mov_imm64(Reg::Rax, 87);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative_except_errno(
            span,
            -2,
            &format!("{name} failed to delete file"),
        );
    }

    fn emit_runtime_path_exists(&mut self, path: &str) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_runtime_path_exists_label(path_label);
    }

    fn emit_runtime_path_exists_label(&mut self, path_label: DataLabel) {
        self.asm.mov_imm64(Reg::Rax, 21);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.mov_imm64(Reg::Rsi, 0);
        self.asm.syscall();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.setcc_al(Condition::Equal);
        self.asm.movzx_rax_al();
    }

    fn emit_runtime_path_type_check(
        &mut self,
        path: &str,
        directory: bool,
        span: Span,
        name: &str,
    ) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_runtime_path_type_check_label(path_label, directory, span, name);
    }

    fn emit_runtime_path_type_check_label(
        &mut self,
        path_label: DataLabel,
        directory: bool,
        span: Span,
        name: &str,
    ) {
        let stat_buffer = self.asm.data_label_with_bytes(&[0; 144]);
        let not_found = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.mov_imm64(Reg::Rax, 262);
        self.asm.mov_imm64(Reg::Rdi, (-100i64) as u64);
        self.asm.mov_data_addr(Reg::Rsi, path_label);
        self.asm.mov_data_addr(Reg::Rdx, stat_buffer);
        self.asm.mov_imm64(Reg::R10, 0);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative_except_errno(
            span,
            -2,
            &format!("{name} failed to stat path"),
        );
        self.asm.cmp_reg_imm8(Reg::Rax, -2);
        self.asm.jcc_label(Condition::Equal, not_found);
        self.asm.mov_data_addr(Reg::Rdx, stat_buffer);
        self.asm.load_ptr_disp32(Reg::Rax, Reg::Rdx, 24);
        self.asm.mov_imm64(Reg::R10, 0o170000);
        self.asm.and_reg_reg(Reg::Rax, Reg::R10);
        self.asm
            .mov_imm64(Reg::R10, if directory { 0o040000 } else { 0o100000 });
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.setcc_al(Condition::Equal);
        self.asm.movzx_rax_al();
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_found);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done);
    }

    fn emit_file_copy(&mut self, source: &str, target: &str, span: Span, name: &str) {
        let source_label = self.nul_terminated_data_label(source);
        let target_label = self.nul_terminated_data_label(target);
        self.emit_file_copy_path_labels(source_label, target_label, span, name);
    }

    fn emit_file_copy_path_labels(
        &mut self,
        source_label: DataLabel,
        target_label: DataLabel,
        span: Span,
        name: &str,
    ) {
        self.asm.mov_imm64(Reg::Rax, 2);
        self.asm.mov_data_addr(Reg::Rdi, source_label);
        self.asm.mov_imm64(Reg::Rsi, 0);
        self.asm.mov_imm64(Reg::Rdx, 0);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(
            span,
            &format!("{name} failed to open source file"),
        );
        self.asm.mov_reg_reg(Reg::R9, Reg::Rax);

        self.asm.mov_imm64(Reg::Rax, 2);
        self.asm.mov_data_addr(Reg::Rdi, target_label);
        self.asm.mov_imm64(Reg::Rsi, 1 | 64 | 512);
        self.asm.mov_imm64(Reg::Rdx, 0o644);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(
            span,
            &format!("{name} failed to open target file"),
        );
        self.asm.mov_reg_reg(Reg::R8, Reg::Rax);

        let copy_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(copy_loop);
        self.asm.mov_imm64(Reg::Rax, 40);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R8);
        self.asm.mov_reg_reg(Reg::Rsi, Reg::R9);
        self.asm.mov_imm64(Reg::Rdx, 0);
        self.asm.mov_imm64(Reg::R10, 0x7ffff000);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to copy file"));
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.jmp_label(copy_loop);
        self.asm.bind_text_label(done);

        self.asm.mov_imm64(Reg::Rax, 3);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R8);
        self.asm.syscall();
        self.asm.mov_imm64(Reg::Rax, 3);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R9);
        self.asm.syscall();
    }

    fn emit_file_stream_to_fd(&mut self, path: &str, fd: u64, span: Span, name: &str) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_file_stream_to_fd_path_label(path_label, fd, span, name);
    }

    fn emit_file_stream_to_fd_path_label(
        &mut self,
        path_label: DataLabel,
        fd: u64,
        span: Span,
        name: &str,
    ) {
        let buffer = self.asm.data_label_with_bytes(&[0; 8192]);
        self.asm.mov_imm64(Reg::Rax, 2);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.mov_imm64(Reg::Rsi, 0);
        self.asm.mov_imm64(Reg::Rdx, 0);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to open file"));
        self.asm.mov_reg_reg(Reg::R9, Reg::Rax);

        let read_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(read_loop);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R9);
        self.asm.mov_data_addr(Reg::Rsi, buffer);
        self.asm.mov_imm64(Reg::Rdx, 8192);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to read file"));
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_reg_reg(Reg::R8, Reg::Rax);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.mov_imm64(Reg::Rdi, fd);
        self.asm.mov_data_addr(Reg::Rsi, buffer);
        self.asm.mov_reg_reg(Reg::Rdx, Reg::R8);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to write output"));
        self.asm.jmp_label(read_loop);
        self.asm.bind_text_label(done);

        self.asm.mov_imm64(Reg::Rax, 3);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R9);
        self.asm.syscall();
    }

    fn emit_file_read_to_runtime_string(
        &mut self,
        path: &str,
        span: Span,
        name: &str,
    ) -> NativeValue {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_file_read_to_runtime_string_from_path_label(path_label, span, name)
    }

    fn emit_file_read_to_runtime_string_from_path_label(
        &mut self,
        path_label: DataLabel,
        span: Span,
        name: &str,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let overflow = self.asm.data_label_with_bytes(&[0]);
        let len = self.asm.data_label_with_i64s(&[0]);
        self.asm.mov_imm64(Reg::Rax, 2);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.mov_imm64(Reg::Rsi, 0);
        self.asm.mov_imm64(Reg::Rdx, 0);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to open file"));
        self.asm.mov_reg_reg(Reg::R9, Reg::Rax);

        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R9);
        self.asm.mov_data_addr(Reg::Rsi, data);
        self.asm.mov_imm64(Reg::Rdx, RUNTIME_STRING_CAP as u64);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to read file"));
        self.asm.mov_data_addr(Reg::Rcx, len);
        self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::Rax);
        self.asm.mov_imm64(Reg::Rcx, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rcx);
        let close_file = self.asm.create_text_label();
        self.asm.jcc_label(Condition::NotEqual, close_file);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R9);
        self.asm.mov_data_addr(Reg::Rsi, overflow);
        self.asm.mov_imm64(Reg::Rdx, 1);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to read file"));
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, close_file);
        self.emit_runtime_error(span, &format!("{name} runtime string exceeds 65536 bytes"));

        self.asm.bind_text_label(close_file);
        self.asm.mov_imm64(Reg::Rax, 3);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R9);
        self.asm.syscall();
        NativeValue::RuntimeString { data, len }
    }

    fn compile_dir_current(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#current", arguments, 0, span)?;
        Ok(self.emit_dir_current(span))
    }

    fn emit_dir_current(&mut self, span: Span) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self
            .asm
            .data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP + 1]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_imm64(Reg::Rax, 79);
        self.asm.mov_data_addr(Reg::Rdi, data);
        self.asm
            .mov_imm64(Reg::Rsi, (RUNTIME_STRING_CAP + 1) as u64);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, "Dir#current failed");
        self.asm.dec_reg(Reg::Rax);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rax);

        NativeValue::RuntimeString { data, len }
    }

    fn compile_dir_home(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#home", arguments, 0, span)?;
        Ok(self.emit_environment_get_static_key(
            "HOME",
            span,
            "failed to get home dir: environment variable HOME is not set",
        ))
    }

    fn compile_dir_temp(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#temp", arguments, 0, span)?;
        Ok(self.emit_environment_get_static_key_or_default("TMPDIR", "/tmp", span))
    }

    fn compile_dir_exists(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#exists", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "Dir#exists")?;
            self.emit_runtime_path_exists_label(path_label);
            return Ok(NativeValue::Bool);
        }
        let path =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, "Dir#exists")?;
        if self.unknown_virtual_paths.contains(&path) {
            self.emit_runtime_path_exists(&path);
        } else {
            self.asm
                .mov_imm64(Reg::Rax, u64::from(self.native_path_exists(&path)));
        }
        Ok(NativeValue::Bool)
    }

    fn compile_dir_is_directory(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#isDirectory", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "Dir#isDirectory")?;
            self.emit_runtime_path_type_check_label(path_label, true, span, "Dir#isDirectory");
            return Ok(NativeValue::Bool);
        }
        let path = self.static_string_from_argument_preserving_effects(
            &arguments[0],
            span,
            "Dir#isDirectory",
        )?;
        if self.unknown_virtual_paths.contains(&path) {
            self.emit_runtime_path_type_check(&path, true, span, "Dir#isDirectory");
        } else {
            self.asm
                .mov_imm64(Reg::Rax, u64::from(self.native_path_is_dir(&path)));
        }
        Ok(NativeValue::Bool)
    }

    fn compile_dir_is_file(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#isFile", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "Dir#isFile")?;
            self.emit_runtime_path_type_check_label(path_label, false, span, "Dir#isFile");
            return Ok(NativeValue::Bool);
        }
        let path =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, "Dir#isFile")?;
        if self.unknown_virtual_paths.contains(&path) {
            self.emit_runtime_path_type_check(&path, false, span, "Dir#isFile");
        } else {
            self.asm
                .mov_imm64(Reg::Rax, u64::from(self.native_path_is_file(&path)));
        }
        Ok(NativeValue::Bool)
    }

    fn compile_dir_mkdir(
        &mut self,
        arguments: &[Expr],
        recursive: bool,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let name = if recursive { "Dir#mkdirs" } else { "Dir#mkdir" };
        self.expect_static_arity(name, arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label = self.compile_runtime_path_argument(&arguments[0], span, name)?;
            if recursive {
                self.emit_dir_mkdirs_label(path_label, span, name);
            } else {
                self.emit_dir_mkdir_label(path_label, recursive, span, name);
            }
            return Ok(NativeValue::Unit);
        }
        let path =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
        let paths = if recursive {
            mkdir_prefixes(&path)
        } else {
            vec![path]
        };
        for path in paths {
            self.emit_dir_mkdir(&path, recursive, span, name);
            self.unknown_virtual_paths.remove(&path);
            self.virtual_dirs.insert(path);
        }
        Ok(NativeValue::Unit)
    }

    fn compile_dir_list(
        &mut self,
        arguments: &[Expr],
        full: bool,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let name = if full { "Dir#listFull" } else { "Dir#list" };
        self.expect_static_arity(name, arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let (path_label, path) =
                self.compile_runtime_path_argument_ref(&arguments[0], span, name)?;
            return Ok(self.emit_dir_list_path_label(path_label, path, full, span, name));
        }
        let path =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, name)?;
        if self.virtual_dir_state_unknown(&path) {
            let path_label = self.nul_terminated_data_label(&path);
            let path_data = self.asm.data_label_with_bytes(path.as_bytes());
            return Ok(self.emit_dir_list_path_label(
                path_label,
                NativeStringRef {
                    data: path_data,
                    len: NativeStringLen::Immediate(path.len()),
                },
                full,
                span,
                name,
            ));
        }
        self.ensure_virtual_dir_state_known(&path, span, name)?;
        let mut entries = Vec::new();
        if std::path::Path::new(&path).exists() {
            let read_dir = fs::read_dir(&path).map_err(|error| {
                Diagnostic::compile(span, format!("native {name} failed: {error}"))
            })?;
            for entry in read_dir {
                let entry = entry.map_err(|error| {
                    Diagnostic::compile(span, format!("native {name} failed: {error}"))
                })?;
                let value = if full {
                    entry.path().display().to_string()
                } else {
                    entry.file_name().to_string_lossy().into_owned()
                };
                entries.push(value);
            }
        } else if !self.virtual_dirs.contains(&path) {
            return Err(Diagnostic::compile(
                span,
                format!("native {name} failed: directory does not exist"),
            ));
        }
        entries.extend(self.virtual_dir_entries(&path, full));
        entries.sort();
        entries.dedup();
        let values = entries
            .into_iter()
            .map(|entry| self.static_string_value(entry))
            .collect();
        let label = self.intern_static_list(values);
        Ok(NativeValue::StaticList { label })
    }

    fn compile_dir_delete(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#delete", arguments, 1, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0]) {
            let path_label =
                self.compile_runtime_path_argument(&arguments[0], span, "Dir#delete")?;
            self.emit_dir_delete_label(path_label, span, "Dir#delete");
            return Ok(NativeValue::Unit);
        }
        let path =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, "Dir#delete")?;
        self.emit_dir_delete(&path, span, "Dir#delete");
        self.unknown_virtual_paths.remove(&path);
        self.virtual_dirs.remove(&path);
        Ok(NativeValue::Unit)
    }

    fn compile_dir_copy(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#copy", arguments, 2, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0])
            || self.expr_may_yield_runtime_string(&arguments[1])
        {
            let (source_label, _source) =
                self.compile_path_argument_to_label(&arguments[0], span, "Dir#copy")?;
            let (target_label, target) =
                self.compile_path_argument_to_label(&arguments[1], span, "Dir#copy")?;
            self.emit_file_copy_path_labels(source_label, target_label, span, "Dir#copy");
            if let Some(target) = target {
                self.virtual_files.remove(&target);
                self.unknown_virtual_paths.insert(target);
            }
            return Ok(NativeValue::Unit);
        }
        let source =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, "Dir#copy")?;
        let target =
            self.static_string_from_argument_preserving_effects(&arguments[1], span, "Dir#copy")?;
        if let Some(content) = self.virtual_files.get(&source).cloned() {
            self.emit_file_write(&target, content.as_bytes(), false, span, "Dir#copy");
            self.unknown_virtual_paths.remove(&target);
            self.virtual_files.insert(target, content);
        } else {
            self.emit_file_copy(&source, &target, span, "Dir#copy");
            self.virtual_files.remove(&target);
            self.unknown_virtual_paths.insert(target);
        }
        Ok(NativeValue::Unit)
    }

    fn compile_dir_move(
        &mut self,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity("Dir#move", arguments, 2, span)?;
        if self.expr_may_yield_runtime_string(&arguments[0])
            || self.expr_may_yield_runtime_string(&arguments[1])
        {
            let (source_label, source) =
                self.compile_path_argument_to_label(&arguments[0], span, "Dir#move")?;
            let (target_label, target) =
                self.compile_path_argument_to_label(&arguments[1], span, "Dir#move")?;
            self.emit_dir_move_labels(source_label, target_label, span, "Dir#move");
            if let Some(source) = source {
                self.virtual_files.remove(&source);
                self.virtual_dirs.remove(&source);
                self.unknown_virtual_paths.insert(source);
            }
            if let Some(target) = target {
                self.virtual_files.remove(&target);
                self.virtual_dirs.remove(&target);
                self.unknown_virtual_paths.insert(target);
            }
            return Ok(NativeValue::Unit);
        }
        let source =
            self.static_string_from_argument_preserving_effects(&arguments[0], span, "Dir#move")?;
        let target =
            self.static_string_from_argument_preserving_effects(&arguments[1], span, "Dir#move")?;
        let moved_file = self.virtual_files.remove(&source);
        let moved_dir = self.virtual_dirs.remove(&source);
        self.emit_dir_move(&source, &target, span, "Dir#move");
        if let Some(content) = moved_file {
            self.virtual_files.insert(target.clone(), content);
            self.unknown_virtual_paths.remove(&source);
            self.unknown_virtual_paths.remove(&target);
        } else if moved_dir {
            self.virtual_dirs.insert(target.clone());
            self.unknown_virtual_paths.remove(&source);
            self.unknown_virtual_paths.remove(&target);
        } else {
            self.virtual_files.remove(&target);
            self.virtual_dirs.remove(&target);
            self.unknown_virtual_paths.insert(source);
            self.unknown_virtual_paths.insert(target);
        }
        Ok(NativeValue::Unit)
    }

    fn ensure_virtual_path_known(
        &self,
        path: &str,
        span: Span,
        name: &str,
    ) -> Result<(), Diagnostic> {
        if self.unknown_virtual_paths.contains(path) {
            Err(unsupported(
                span,
                &format!("native {name} after divergent dynamic file state"),
            ))
        } else {
            Ok(())
        }
    }

    fn ensure_virtual_dir_state_known(
        &self,
        path: &str,
        span: Span,
        name: &str,
    ) -> Result<(), Diagnostic> {
        if self.virtual_dir_state_unknown(path) {
            Err(unsupported(
                span,
                &format!("native {name} after divergent dynamic directory state"),
            ))
        } else {
            Ok(())
        }
    }

    fn virtual_dir_state_unknown(&self, path: &str) -> bool {
        self.unknown_virtual_paths
            .iter()
            .any(|unknown| unknown == path || virtual_dir_entry(path, unknown, false).is_some())
    }

    fn native_path_exists(&self, path: &str) -> bool {
        self.virtual_files.contains_key(path)
            || self.virtual_dirs.contains(path)
            || std::path::Path::new(path).exists()
    }

    fn native_path_is_file(&self, path: &str) -> bool {
        self.virtual_files.contains_key(path) || std::path::Path::new(path).is_file()
    }

    fn native_path_is_dir(&self, path: &str) -> bool {
        self.virtual_dirs.contains(path) || std::path::Path::new(path).is_dir()
    }

    fn virtual_dir_entries(&self, path: &str, full: bool) -> Vec<String> {
        self.virtual_files
            .keys()
            .chain(self.virtual_dirs.iter())
            .filter_map(|entry| virtual_dir_entry(path, entry, full))
            .collect()
    }

    fn emit_dir_list_path_label(
        &mut self,
        path_label: DataLabel,
        path: NativeStringRef,
        full: bool,
        span: Span,
        name: &str,
    ) -> NativeValue {
        const DIR_BUFFER_CAP: usize = 8192;
        const RUNTIME_LIST_CAP: usize = 65_536;

        let dir_buffer = self.asm.data_label_with_bytes(&vec![0; DIR_BUFFER_CAP]);
        let output = self.asm.data_label_with_bytes(&vec![0; RUNTIME_LIST_CAP]);
        let sorted_output = self.asm.data_label_with_bytes(&vec![0; RUNTIME_LIST_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let fd_slot = self.asm.data_label_with_i64s(&[0]);
        let bytes_read = self.asm.data_label_with_i64s(&[0]);
        let entry_offset = self.asm.data_label_with_i64s(&[0]);
        let output_offset = self.asm.data_label_with_i64s(&[0]);
        let sorted_output_offset = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rax, output_offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);

        self.asm.mov_imm64(Reg::Rax, 2);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.mov_imm64(Reg::Rsi, 0o200000);
        self.asm.mov_imm64(Reg::Rdx, 0);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to open directory"));
        self.asm.mov_data_addr(Reg::R10, fd_slot);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rax);

        let read_loop = self.asm.create_text_label();
        let entry_loop = self.asm.create_text_label();
        let append_entry = self.asm.create_text_label();
        let next_entry = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(read_loop);
        self.asm.mov_imm64(Reg::Rax, 217);
        self.asm.mov_data_addr(Reg::R10, fd_slot);
        self.asm.load_ptr_disp32(Reg::Rdi, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rsi, dir_buffer);
        self.asm.mov_imm64(Reg::Rdx, DIR_BUFFER_CAP as u64);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to read directory"));
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_data_addr(Reg::R10, bytes_read);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rax);
        self.asm.mov_data_addr(Reg::R10, entry_offset);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.bind_text_label(entry_loop);
        self.asm.mov_data_addr(Reg::R10, entry_offset);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, bytes_read);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, read_loop);
        self.asm.mov_data_addr(Reg::Rsi, dir_buffer);
        self.asm.add_reg_reg(Reg::Rsi, Reg::R8);

        self.asm.movzx_byte_disp32(Reg::Rax, Reg::Rsi, 19);
        self.asm.cmp_reg_imm8(Reg::Rax, b'.' as i8);
        self.asm.jcc_label(Condition::NotEqual, append_entry);
        self.asm.movzx_byte_disp32(Reg::Rax, Reg::Rsi, 20);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, next_entry);
        self.asm.cmp_reg_imm8(Reg::Rax, b'.' as i8);
        self.asm.jcc_label(Condition::NotEqual, append_entry);
        self.asm.movzx_byte_disp32(Reg::Rax, Reg::Rsi, 21);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, next_entry);

        self.asm.bind_text_label(append_entry);
        self.emit_append_newline_separator_to_runtime_buffer_offset_label(
            output,
            output_offset,
            span,
            &format!("{name} result exceeds 65536 bytes"),
        );
        if full {
            self.emit_append_dir_list_full_prefix(
                output,
                output_offset,
                path,
                span,
                &format!("{name} result exceeds 65536 bytes"),
            );
        }
        self.asm.mov_data_addr(Reg::R10, entry_offset);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rsi, dir_buffer);
        self.asm.add_reg_reg(Reg::Rsi, Reg::R8);
        self.asm.add_reg_imm32(Reg::Rsi, 19);
        self.emit_append_c_string_pointer_to_runtime_buffer_offset_label(
            output,
            output_offset,
            span,
            &format!("{name} result exceeds 65536 bytes"),
        );

        self.asm.bind_text_label(next_entry);
        self.asm.mov_data_addr(Reg::R10, entry_offset);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rsi, dir_buffer);
        self.asm.add_reg_reg(Reg::Rsi, Reg::R8);
        self.asm.load_ptr_disp32(Reg::Rax, Reg::Rsi, 16);
        self.asm.mov_imm64(Reg::R10, 0xffff);
        self.asm.and_reg_reg(Reg::Rax, Reg::R10);
        self.asm.add_reg_reg(Reg::R8, Reg::Rax);
        self.asm.mov_data_addr(Reg::R10, entry_offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(entry_loop);

        self.asm.bind_text_label(done);
        self.asm.mov_imm64(Reg::Rax, 3);
        self.asm.mov_data_addr(Reg::R10, fd_slot);
        self.asm.load_ptr_disp32(Reg::Rdi, Reg::R10, 0);
        self.asm.syscall();
        self.emit_sort_runtime_lines(
            output,
            output_offset,
            sorted_output,
            sorted_output_offset,
            span,
            &format!("{name} sorted result exceeds 65536 bytes"),
        );
        self.asm.mov_data_addr(Reg::R10, sorted_output_offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        NativeValue::RuntimeLinesList {
            data: sorted_output,
            len,
        }
    }

    fn emit_sort_runtime_lines(
        &mut self,
        input: DataLabel,
        input_len: DataLabel,
        output: DataLabel,
        output_offset: DataLabel,
        span: Span,
        overflow_message: &str,
    ) {
        const RUNTIME_LINES_CAP: usize = 65_536;

        let selected_start = self.asm.data_label_with_i64s(&[0]);
        let selected_end = self.asm.data_label_with_i64s(&[0]);
        let scan_pos = self.asm.data_label_with_i64s(&[0]);
        let current_start = self.asm.data_label_with_i64s(&[0]);
        let current_end = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::R10, output_offset);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        let select_next = self.asm.create_text_label();
        let scan_loop = self.asm.create_text_label();
        let find_current_end = self.asm.create_text_label();
        let current_end_found = self.asm.create_text_label();
        let maybe_update_selected = self.asm.create_text_label();
        let compare_loop = self.asm.create_text_label();
        let current_at_end = self.asm.create_text_label();
        let selected_at_end = self.asm.create_text_label();
        let update_selected = self.asm.create_text_label();
        let advance_scan = self.asm.create_text_label();
        let append_selected = self.asm.create_text_label();
        let append_without_separator = self.asm.create_text_label();
        let copy_selected = self.asm.create_text_label();
        let mark_selected = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(select_next);
        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.mov_imm64(Reg::R8, u64::MAX);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::R10, selected_end);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::R10, scan_pos);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.bind_text_label(scan_loop);
        self.asm.mov_data_addr(Reg::R10, scan_pos);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, input_len);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, append_selected);

        self.asm.mov_data_addr(Reg::R10, current_start);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::Rsi, input);

        self.asm.bind_text_label(find_current_end);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, current_end_found);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, current_end_found);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(find_current_end);

        self.asm.bind_text_label(current_end_found);
        self.asm.mov_data_addr(Reg::R10, current_end);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::R10, current_start);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rsi, input);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R9);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, advance_scan);

        self.asm.bind_text_label(maybe_update_selected);
        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.cmp_reg_imm32(Reg::R9, -1);
        self.asm.jcc_label(Condition::Equal, update_selected);

        self.asm.mov_imm64(Reg::Rcx, 0);
        self.asm.bind_text_label(compare_loop);
        self.asm.mov_data_addr(Reg::R10, current_start);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.add_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.mov_data_addr(Reg::R10, current_end);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, current_at_end);

        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.add_reg_reg(Reg::R9, Reg::Rcx);
        self.asm.mov_data_addr(Reg::R10, selected_end);
        self.asm.load_ptr_disp32(Reg::R10, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::GreaterEqual, selected_at_end);

        self.asm.mov_data_addr(Reg::Rsi, input);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rbx, Reg::Rsi, Reg::R9);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::Below, update_selected);
        self.asm.jcc_label(Condition::Above, advance_scan);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(compare_loop);

        self.asm.bind_text_label(current_at_end);
        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.add_reg_reg(Reg::R9, Reg::Rcx);
        self.asm.mov_data_addr(Reg::R10, selected_end);
        self.asm.load_ptr_disp32(Reg::R10, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::Less, update_selected);
        self.asm.jmp_label(advance_scan);

        self.asm.bind_text_label(selected_at_end);
        self.asm.jmp_label(advance_scan);

        self.asm.bind_text_label(update_selected);
        self.asm.mov_data_addr(Reg::R10, current_start);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::R10, current_end);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, selected_end);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.bind_text_label(advance_scan);
        self.asm.mov_data_addr(Reg::R10, current_end);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, input_len);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, append_selected);
        self.asm.inc_reg(Reg::R8);
        self.asm.mov_data_addr(Reg::R10, scan_pos);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(scan_loop);

        self.asm.bind_text_label(append_selected);
        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.cmp_reg_imm32(Reg::R8, -1);
        self.asm.jcc_label(Condition::Equal, done);

        self.asm.mov_data_addr(Reg::R10, output_offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::R9, 0);
        self.asm
            .jcc_label(Condition::Equal, append_without_separator);
        self.emit_runtime_buffer_capacity_check(Reg::R9, RUNTIME_LINES_CAP, span, overflow_message);
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R9);
        self.asm.mov_data_addr(Reg::R10, output_offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        self.asm.bind_text_label(append_without_separator);
        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, selected_end);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);

        self.asm.bind_text_label(copy_selected);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, mark_selected);
        self.asm.mov_data_addr(Reg::R10, output_offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.emit_runtime_buffer_capacity_check(Reg::R9, RUNTIME_LINES_CAP, span, overflow_message);
        self.asm.mov_data_addr(Reg::Rsi, input);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.mov_data_addr(Reg::R10, output_offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
        self.asm.jmp_label(copy_selected);

        self.asm.bind_text_label(mark_selected);
        self.asm.mov_data_addr(Reg::R10, selected_start);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rbx, input);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.jmp_label(select_next);

        self.asm.bind_text_label(done);
    }

    fn emit_append_newline_separator_to_runtime_buffer_offset_label(
        &mut self,
        output: DataLabel,
        offset: DataLabel,
        span: Span,
        overflow_message: &str,
    ) {
        let done = self.asm.create_text_label();
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::R9, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R9, 65_536, span, overflow_message);
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R9);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
        self.asm.bind_text_label(done);
    }

    fn emit_append_dir_list_full_prefix(
        &mut self,
        output: DataLabel,
        offset: DataLabel,
        path: NativeStringRef,
        span: Span,
        overflow_message: &str,
    ) {
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            output,
            offset,
            path,
            span,
            overflow_message,
        );

        let done = self.asm.create_text_label();
        self.emit_load_native_string_len(Reg::Rdx, path.len);
        self.asm.cmp_reg_imm8(Reg::Rdx, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_data_addr(Reg::Rsi, path.data);
        self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.dec_reg(Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'/' as i8);
        self.asm.jcc_label(Condition::Equal, done);
        let slash = self.asm.data_label_with_bytes(b"/");
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            output,
            offset,
            NativeStringRef {
                data: slash,
                len: NativeStringLen::Immediate(1),
            },
            span,
            overflow_message,
        );
        self.asm.bind_text_label(done);
    }

    fn emit_append_c_string_pointer_to_runtime_buffer_offset_label(
        &mut self,
        output: DataLabel,
        offset: DataLabel,
        span: Span,
        overflow_message: &str,
    ) {
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R9, 65_536, span, overflow_message);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
    }

    fn emit_dir_mkdir(&mut self, path: &str, recursive: bool, span: Span, name: &str) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_dir_mkdir_label(path_label, recursive, span, name);
    }

    fn emit_dir_mkdir_label(
        &mut self,
        path_label: DataLabel,
        recursive: bool,
        span: Span,
        name: &str,
    ) {
        self.asm.mov_imm64(Reg::Rax, 83);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.mov_imm64(Reg::Rsi, 0o755);
        self.asm.syscall();
        if recursive {
            self.emit_runtime_error_if_rax_negative_except_errno(
                span,
                -17,
                &format!("{name} failed to create directory"),
            );
        } else {
            self.emit_runtime_error_if_rax_negative(
                span,
                &format!("{name} failed to create directory"),
            );
        }
    }

    fn emit_dir_mkdirs_label(&mut self, path_label: DataLabel, span: Span, name: &str) {
        self.asm.mov_data_addr(Reg::Rbx, path_label);
        self.asm.mov_imm64(Reg::R8, 1);
        let scan = self.asm.create_text_label();
        let mkdir_prefix = self.asm.create_text_label();
        let mkdir_final = self.asm.create_text_label();

        self.asm.bind_text_label(scan);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rbx, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, mkdir_final);
        self.asm.cmp_reg_imm8(Reg::Rax, b'/' as i8);
        self.asm.jcc_label(Condition::Equal, mkdir_prefix);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(mkdir_prefix);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.emit_dir_mkdir_label(path_label, true, span, name);
        self.asm.mov_imm64(Reg::Rax, b'/' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(mkdir_final);
        self.emit_dir_mkdir_label(path_label, true, span, name);
    }

    fn emit_dir_delete(&mut self, path: &str, span: Span, name: &str) {
        let path_label = self.nul_terminated_data_label(path);
        self.emit_dir_delete_label(path_label, span, name);
    }

    fn emit_dir_delete_label(&mut self, path_label: DataLabel, span: Span, name: &str) {
        self.asm.mov_imm64(Reg::Rax, 84);
        self.asm.mov_data_addr(Reg::Rdi, path_label);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(
            span,
            &format!("{name} failed to delete directory"),
        );
    }

    fn emit_dir_move(&mut self, source: &str, target: &str, span: Span, name: &str) {
        let source_label = self.nul_terminated_data_label(source);
        let target_label = self.nul_terminated_data_label(target);
        self.emit_dir_move_labels(source_label, target_label, span, name);
    }

    fn emit_dir_move_labels(
        &mut self,
        source_label: DataLabel,
        target_label: DataLabel,
        span: Span,
        name: &str,
    ) {
        self.asm.mov_imm64(Reg::Rax, 82);
        self.asm.mov_data_addr(Reg::Rdi, source_label);
        self.asm.mov_data_addr(Reg::Rsi, target_label);
        self.asm.syscall();
        self.emit_runtime_error_if_rax_negative(span, &format!("{name} failed to move path"));
    }

    fn nul_terminated_data_label(&mut self, value: &str) -> DataLabel {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        self.asm.data_label_with_bytes(&bytes)
    }

    fn compile_map_literal(
        &mut self,
        entries: &[(Expr, Expr)],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let mut static_entries = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            let key = self.static_value_from_argument_preserving_effects(
                key,
                span,
                "native map key with non-static value",
            )?;
            let value = self.static_value_from_argument_preserving_effects(
                value,
                span,
                "native map value with non-static value",
            )?;
            static_entries.push((key, value));
        }
        let label = self.intern_static_map(static_entries);
        Ok(NativeValue::StaticMap { label })
    }

    fn compile_set_literal(
        &mut self,
        elements: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let mut unique = Vec::new();
        for element in elements {
            let value = self.static_value_from_argument_preserving_effects(
                element,
                span,
                "native set element with non-static value",
            )?;
            if !unique
                .iter()
                .any(|existing| self.static_value_equal_user(existing, &value))
            {
                unique.push(value);
            }
        }
        let label = self.intern_static_set(unique);
        Ok(NativeValue::StaticSet { label })
    }

    fn compile_record_literal(
        &mut self,
        name: &str,
        fields: &[(String, Expr)],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let mut fields_out = Vec::with_capacity(fields.len());
        for (field, value) in fields {
            let value = self.static_value_from_argument_preserving_effects(
                value,
                span,
                "native record field with non-static value",
            )?;
            fields_out.push((field.clone(), value));
        }
        let label = self.intern_static_record(name.to_string(), fields_out);
        Ok(NativeValue::StaticRecord { label })
    }

    fn compile_record_constructor(
        &mut self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let fields =
            self.record_schemas.get(name).cloned().ok_or_else(|| {
                Diagnostic::compile(span, format!("unknown native record `{name}`"))
            })?;
        if fields.len() != arguments.len() {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "record `{name}` expects {} arguments but got {}",
                    fields.len(),
                    arguments.len()
                ),
            ));
        }
        let mut fields_out = Vec::with_capacity(fields.len());
        for (field, value) in fields.into_iter().zip(arguments.iter()) {
            let value = self.static_value_from_argument_preserving_effects(
                value,
                span,
                "native record field with non-static value",
            )?;
            fields_out.push((field, value));
        }
        let label = self.intern_static_record(name.to_string(), fields_out);
        Ok(NativeValue::StaticRecord { label })
    }

    fn compile_field_access(
        &mut self,
        target: &Expr,
        field: &str,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let target = self.compile_expr(target)?;
        let NativeValue::StaticRecord { label } = target else {
            return Err(unsupported(
                span,
                "native field access for non-static record",
            ));
        };
        let value = self
            .static_record_field(label, field)
            .ok_or_else(|| Diagnostic::compile(span, format!("record has no field `{field}`")))?;
        Ok(self.emit_static_value(&value))
    }

    fn static_value_from_expr(&mut self, expr: &Expr) -> Option<StaticValue> {
        match expr {
            Expr::Int { value, .. } => Some(StaticValue::Int(*value)),
            Expr::Double { value, kind, .. } => match kind {
                FloatLiteralKind::Float => Some(StaticValue::Float((*value as f32).to_bits())),
                FloatLiteralKind::Double => Some(StaticValue::Double(value.to_bits())),
            },
            Expr::Bool { value, .. } => Some(StaticValue::Bool(*value)),
            Expr::Null { .. } => Some(StaticValue::Null),
            Expr::Unit { .. } => Some(StaticValue::Unit),
            Expr::String { value, .. } => {
                let value = if value.contains("#{") {
                    self.static_interpolated_string_value(value)?
                } else {
                    value.clone()
                };
                let label = self.asm.data_label_with_bytes(value.as_bytes());
                Some(StaticValue::StaticString {
                    label,
                    len: value.len(),
                })
            }
            Expr::ListLiteral { elements, .. } => {
                if let Some(values) = elements
                    .iter()
                    .map(const_int_expr)
                    .collect::<Option<Vec<_>>>()
                {
                    let label = self.asm.data_label_with_i64s(&values);
                    Some(StaticValue::StaticIntList {
                        label,
                        len: values.len(),
                    })
                } else {
                    let elements = self.static_list_elements_from_exprs(elements)?;
                    Some(self.static_list_value_from_elements(elements))
                }
            }
            Expr::MapLiteral { entries, .. } => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| {
                        let key = self.static_value_from_expr(key)?;
                        let value = self.static_value_from_expr(value)?;
                        Some((key, value))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let label = self.intern_static_map(entries);
                Some(StaticValue::StaticMap { label })
            }
            Expr::SetLiteral { elements, .. } => {
                let elements = self.static_set_elements_from_exprs(elements)?;
                let label = self.intern_static_set(elements);
                Some(StaticValue::StaticSet { label })
            }
            Expr::Identifier { name, .. } => self.lookup_static_value(name).or_else(|| {
                self.lookup_var(name)
                    .and_then(|slot| self.static_value_from_native(slot.value))
                    .or_else(|| self.static_lambda_value_for_function_name(name))
                    .or_else(|| self.static_builtin_value_for_identifier(name))
            }),
            Expr::FieldAccess { target, field, .. } => {
                let StaticValue::StaticRecord { label } = self.static_value_from_expr(target)?
                else {
                    return None;
                };
                self.static_record_field(label, field)
            }
            Expr::RecordLiteral { fields, .. } => {
                let fields = fields
                    .iter()
                    .map(|(field, value)| {
                        self.static_value_from_expr(value)
                            .map(|value| (field.clone(), value))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let label = self.intern_static_record(String::new(), fields);
                Some(StaticValue::StaticRecord { label })
            }
            Expr::RecordConstructor {
                name, arguments, ..
            } => {
                let fields = self.record_schemas.get(name)?.clone();
                if fields.len() != arguments.len() {
                    return None;
                }
                let fields = fields
                    .into_iter()
                    .zip(arguments.iter())
                    .map(|(field, value)| {
                        self.static_value_from_expr(value)
                            .map(|value| (field, value))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let label = self.intern_static_record(name.clone(), fields);
                Some(StaticValue::StaticRecord { label })
            }
            Expr::Lambda { params, body, .. } => {
                let thread_aliases = self.current_thread_aliases();
                let label = self.intern_static_lambda(
                    params.clone(),
                    body.as_ref().clone(),
                    self.current_static_captures(),
                    self.current_runtime_captures(),
                    expr_contains_thread_call(body, &thread_aliases),
                );
                Some(StaticValue::StaticLambda { label })
            }
            Expr::VarDecl {
                name,
                value,
                mutable: false,
                ..
            } => {
                let value = self.static_value_from_expr(value)?;
                self.bind_static_value(name.clone(), value);
                Some(StaticValue::Unit)
            }
            Expr::VarDecl { .. } => None,
            Expr::Assign { name, value, .. } => {
                let value = self.static_value_from_expr(value)?;
                self.assign_static_value(name, value.clone());
                Some(value)
            }
            Expr::Block { expressions, .. } => {
                self.push_scope();
                let result = (|| {
                    let mut last = StaticValue::Unit;
                    for expression in expressions {
                        last = self.static_value_from_expr(expression)?;
                    }
                    Some(last)
                })();
                self.pop_scope();
                result
            }
            Expr::Unary {
                op: UnaryOp::Plus,
                expr,
                ..
            } => self.static_value_from_expr(expr),
            Expr::Unary {
                op: UnaryOp::Minus,
                expr,
                ..
            } => match self.static_value_from_expr(expr)? {
                StaticValue::Int(value) => value.checked_neg().map(StaticValue::Int),
                StaticValue::Float(bits) => {
                    Some(StaticValue::Float((-f32::from_bits(bits)).to_bits()))
                }
                StaticValue::Double(bits) => {
                    Some(StaticValue::Double((-f64::from_bits(bits)).to_bits()))
                }
                _ => None,
            },
            Expr::Unary {
                op: UnaryOp::Not,
                expr,
                ..
            } => {
                let StaticValue::Bool(value) = self.static_value_from_expr(expr)? else {
                    return None;
                };
                Some(StaticValue::Bool(!value))
            }
            Expr::Binary { lhs, op, rhs, .. } => {
                if *op == BinaryOp::Add
                    && let Some(value) = self.static_string_concat_text(lhs, rhs)
                {
                    return Some(self.static_string_value(value));
                }
                if matches!(op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
                    let StaticValue::Bool(lhs) = self.static_value_from_expr(lhs)? else {
                        return None;
                    };
                    match (op, lhs) {
                        (BinaryOp::LogicalAnd, false) => return Some(StaticValue::Bool(false)),
                        (BinaryOp::LogicalOr, true) => return Some(StaticValue::Bool(true)),
                        _ => {}
                    }
                    let StaticValue::Bool(rhs) = self.static_value_from_expr(rhs)? else {
                        return None;
                    };
                    return Some(StaticValue::Bool(rhs));
                }
                let lhs = self.static_value_from_expr(lhs)?;
                let rhs = self.static_value_from_expr(rhs)?;
                match (lhs, op, rhs) {
                    (StaticValue::Int(lhs), BinaryOp::Add, StaticValue::Int(rhs)) => {
                        lhs.checked_add(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::Subtract, StaticValue::Int(rhs)) => {
                        lhs.checked_sub(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::Multiply, StaticValue::Int(rhs)) => {
                        lhs.checked_mul(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::Divide, StaticValue::Int(rhs))
                        if rhs != 0 =>
                    {
                        lhs.checked_div(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::BitAnd, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Int(lhs & rhs))
                    }
                    (StaticValue::Int(lhs), BinaryOp::BitOr, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Int(lhs | rhs))
                    }
                    (StaticValue::Int(lhs), BinaryOp::BitXor, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Int(lhs ^ rhs))
                    }
                    (lhs, op, rhs)
                        if matches!(
                            op,
                            BinaryOp::Add
                                | BinaryOp::Subtract
                                | BinaryOp::Multiply
                                | BinaryOp::Divide
                        ) =>
                    {
                        static_numeric_binary_value(*op, &lhs, &rhs)
                    }
                    (StaticValue::Int(lhs), BinaryOp::Less, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Bool(lhs < rhs))
                    }
                    (StaticValue::Int(lhs), BinaryOp::LessEqual, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Bool(lhs <= rhs))
                    }
                    (StaticValue::Int(lhs), BinaryOp::Greater, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Bool(lhs > rhs))
                    }
                    (StaticValue::Int(lhs), BinaryOp::GreaterEqual, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Bool(lhs >= rhs))
                    }
                    (lhs, BinaryOp::Less, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? < static_value_as_f64(&rhs)?,
                        ))
                    }
                    (lhs, BinaryOp::LessEqual, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? <= static_value_as_f64(&rhs)?,
                        ))
                    }
                    (lhs, BinaryOp::Greater, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? > static_value_as_f64(&rhs)?,
                        ))
                    }
                    (lhs, BinaryOp::GreaterEqual, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? >= static_value_as_f64(&rhs)?,
                        ))
                    }
                    (StaticValue::Bool(lhs), BinaryOp::LogicalAnd, StaticValue::Bool(rhs)) => {
                        Some(StaticValue::Bool(lhs && rhs))
                    }
                    (StaticValue::Bool(lhs), BinaryOp::LogicalOr, StaticValue::Bool(rhs)) => {
                        Some(StaticValue::Bool(lhs || rhs))
                    }
                    (lhs, BinaryOp::Equal, rhs) => {
                        Some(StaticValue::Bool(self.static_value_equal_user(&lhs, &rhs)))
                    }
                    (lhs, BinaryOp::NotEqual, rhs) => {
                        Some(StaticValue::Bool(!self.static_value_equal_user(&lhs, &rhs)))
                    }
                    _ => None,
                }
            }
            Expr::Call {
                callee, arguments, ..
            } => self.static_call_value_from_expr(callee, arguments),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.static_if_value(condition, then_branch, else_branch.as_deref()),
            _ => None,
        }
    }

    fn static_value_from_pure_expr(&mut self, expr: &Expr) -> Option<StaticValue> {
        if static_expr_is_pure(expr) {
            self.static_value_from_expr(expr)
        } else {
            None
        }
    }

    fn static_if_value(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
    ) -> Option<StaticValue> {
        if !static_expr_is_pure(condition) {
            return None;
        }
        let StaticValue::Bool(condition) = self.static_value_from_expr(condition)? else {
            return None;
        };
        if condition {
            if !static_expr_is_pure(then_branch) {
                return None;
            }
            self.static_value_from_expr(then_branch)
        } else {
            let Some(branch) = else_branch else {
                return Some(StaticValue::Unit);
            };
            if !static_expr_is_pure(branch) {
                return None;
            }
            self.static_value_from_expr(branch)
        }
    }

    fn static_call_value_from_expr(
        &mut self,
        callee: &Expr,
        arguments: &[Expr],
    ) -> Option<StaticValue> {
        match callee {
            Expr::Identifier { name, .. } => self.static_call_value_by_name(name, arguments),
            Expr::Call {
                callee,
                arguments: initial_arguments,
                ..
            } => {
                let Expr::Identifier { name, .. } = callee.as_ref() else {
                    return None;
                };
                match name.as_str() {
                    "map" if initial_arguments.len() == 1 && arguments.len() == 1 => self
                        .static_call_value_by_name(
                            "map",
                            &[initial_arguments[0].clone(), arguments[0].clone()],
                        ),
                    "bind" if initial_arguments.len() == 1 && arguments.len() == 1 => self
                        .static_call_value_by_name(
                            "bind",
                            &[initial_arguments[0].clone(), arguments[0].clone()],
                        ),
                    _ => None,
                }
            }
            Expr::FieldAccess { target, field, .. } => {
                let mut lowered = Vec::with_capacity(arguments.len() + 1);
                lowered.push(target.as_ref().clone());
                lowered.extend(arguments.iter().cloned());
                self.static_call_value_by_name(field, &lowered)
            }
            _ => None,
        }
    }

    fn static_function_call_value(
        &mut self,
        function: &NativeFunction,
        arguments: &[Expr],
    ) -> Option<StaticValue> {
        if function.contains_thread_call {
            return None;
        }
        if !static_expr_is_pure(&function.body) {
            return None;
        }
        if arguments.len() != function.params.len() {
            return None;
        }
        let values = arguments
            .iter()
            .map(|argument| self.static_value_from_pure_expr(argument))
            .collect::<Option<Vec<_>>>()?;
        self.push_scope();
        for (param, value) in function.params.iter().zip(values) {
            self.bind_static_value(param.clone(), value);
        }
        let result = self.static_value_from_expr(&function.body);
        self.pop_scope();
        result
    }

    fn static_function_call_value_from_values(
        &mut self,
        function: &NativeFunction,
        arguments: &[StaticValue],
    ) -> Option<StaticValue> {
        if function.contains_thread_call {
            return None;
        }
        if !static_expr_is_pure(&function.body) {
            return None;
        }
        if arguments.len() != function.params.len() {
            return None;
        }
        self.push_scope();
        for (param, value) in function.params.iter().zip(arguments.iter().cloned()) {
            self.bind_static_value(param.clone(), value);
        }
        let result = self.static_value_from_expr(&function.body);
        self.pop_scope();
        result
    }

    fn static_instance_method_call_value(
        &mut self,
        name: &str,
        arguments: &[Expr],
    ) -> Option<StaticValue> {
        let arg_values = arguments
            .iter()
            .map(|argument| self.static_value_from_pure_expr(argument))
            .collect::<Option<Vec<_>>>()?;
        self.static_instance_method_call_value_from_values(name, &arg_values)
    }

    fn static_instance_method_call_value_from_values(
        &mut self,
        name: &str,
        arg_values: &[StaticValue],
    ) -> Option<StaticValue> {
        for method in self.instance_methods.clone() {
            if method.name != name || method.params.len() != arg_values.len() {
                continue;
            }
            if !method
                .param_annotations
                .iter()
                .zip(arg_values.iter())
                .all(|(annotation, value)| self.static_value_matches_annotation(value, annotation))
            {
                continue;
            }
            self.push_scope();
            for (param, value) in method.params.iter().zip(arg_values.iter().cloned()) {
                self.bind_static_value(param.clone(), value);
            }
            let result = self.static_value_from_expr(&method.body);
            self.pop_scope();
            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn static_direct_map_value(&mut self, arguments: &[Expr]) -> Option<StaticValue> {
        let (list, mapper) = if self.static_list_values_from_expr(&arguments[0]).is_some() {
            (&arguments[0], &arguments[1])
        } else {
            (&arguments[1], &arguments[0])
        };
        let elements = self.static_list_values_from_expr(list)?;
        let mapped = elements
            .into_iter()
            .map(|element| self.static_apply_callable_value(mapper, vec![element]))
            .collect::<Option<Vec<_>>>()?;
        Some(self.static_list_value_from_elements(mapped))
    }

    fn static_apply_callable_value(
        &mut self,
        callable: &Expr,
        arguments: Vec<StaticValue>,
    ) -> Option<StaticValue> {
        if let Some(StaticValue::StaticLambda { label }) = self.static_value_from_expr(callable)
            && let Some(value) = self.static_apply_lambda_value(None, label, arguments.clone())
        {
            return Some(value);
        }
        if let Some(StaticValue::BuiltinFunction { label }) = self.static_value_from_expr(callable)
        {
            let name = self.builtin_aliases.get(label.0)?.clone();
            return self.static_call_value_by_name_with_values(&name, &arguments);
        }
        match callable {
            Expr::Lambda { params, body, .. } => {
                let thread_aliases = self.current_thread_aliases();
                if expr_contains_thread_call(body, &thread_aliases) || !static_expr_is_pure(body) {
                    return None;
                }
                if params.len() != arguments.len() {
                    return None;
                }
                let bindings = params
                    .iter()
                    .map(String::as_str)
                    .zip(arguments)
                    .collect::<Vec<_>>();
                self.static_value_from_expr_with_bindings(body, &bindings)
            }
            Expr::Identifier { name, .. } => {
                if let Some(StaticValue::StaticLambda { label }) = self.lookup_static_value(name)
                    && let Some(value) =
                        self.static_apply_lambda_value(None, label, arguments.clone())
                {
                    return Some(value);
                }
                self.static_call_value_by_name_with_values(name, &arguments)
            }
            _ => None,
        }
    }

    fn compile_callable_with_static_arguments_preserving_effects(
        &mut self,
        callable: &Expr,
        arguments: Vec<StaticValue>,
        span: Span,
        feature: &str,
    ) -> Result<StaticValue, Diagnostic> {
        if let Some(value) = self.static_apply_callable_value(callable, arguments.clone()) {
            return Ok(value);
        }
        match callable {
            Expr::Lambda { params, body, .. } => self
                .compile_lambda_body_with_static_arguments_preserving_effects(
                    params, body, arguments, span, feature,
                ),
            Expr::Identifier { name, .. } => {
                if let Some(StaticValue::StaticLambda { label }) = self.lookup_static_value(name) {
                    self.compile_static_lambda_with_static_arguments_preserving_effects(
                        label, arguments, span, feature,
                    )
                } else {
                    Err(unsupported(span, feature))
                }
            }
            _ => Err(unsupported(span, feature)),
        }
    }

    fn compile_static_lambda_with_static_arguments_preserving_effects(
        &mut self,
        label: LambdaLabel,
        arguments: Vec<StaticValue>,
        span: Span,
        feature: &str,
    ) -> Result<StaticValue, Diagnostic> {
        let lambda = self
            .static_lambdas
            .get(label.0)
            .cloned()
            .ok_or_else(|| unsupported(span, feature))?;
        self.push_scope();
        self.bind_static_lambda_captures(&lambda);
        let result = self.compile_lambda_body_with_static_arguments_preserving_effects(
            &lambda.params,
            &lambda.body,
            arguments,
            span,
            feature,
        );
        self.pop_scope();
        result
    }

    fn compile_static_lambda_inline_call(
        &mut self,
        label: LambdaLabel,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let lambda = self
            .static_lambdas
            .get(label.0)
            .cloned()
            .ok_or_else(|| unsupported(span, "native static lambda call"))?;
        if arguments.len() != lambda.params.len() {
            return Err(Diagnostic::compile(
                span,
                format!(
                    "lambda expects {} arguments but got {}",
                    lambda.params.len(),
                    arguments.len()
                ),
            ));
        }
        self.push_scope();
        let result = (|| {
            let assigned_captures = assigned_names_in_expr(&lambda.body);
            self.bind_static_lambda_runtime_captures(&lambda);
            for (name, value) in lambda.captures.clone() {
                if self.static_capture_shadowed_by_runtime(&lambda, &name)
                    || (assigned_captures.contains(&name) && self.lookup_var(&name).is_some())
                {
                    continue;
                }
                self.bind_static_runtime_value(name, value);
            }
            for (param, argument) in lambda.params.iter().zip(arguments) {
                let static_argument = self.static_value_from_pure_expr(argument);
                let value = self.compile_expr(argument)?;
                match value {
                    NativeValue::Int | NativeValue::Bool => {
                        let slot = self.allocate_slot(param.clone(), value);
                        self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                        if let Some(value) = static_argument {
                            self.bind_static_value(param.clone(), value);
                        }
                    }
                    NativeValue::Null
                    | NativeValue::Unit
                    | NativeValue::StaticFloat { .. }
                    | NativeValue::StaticDouble { .. }
                    | NativeValue::StaticString { .. }
                    | NativeValue::RuntimeString { .. }
                    | NativeValue::RuntimeLinesList { .. }
                    | NativeValue::StaticIntList { .. }
                    | NativeValue::StaticList { .. }
                    | NativeValue::StaticRecord { .. }
                    | NativeValue::StaticMap { .. }
                    | NativeValue::StaticSet { .. }
                    | NativeValue::StaticLambda { .. }
                    | NativeValue::BuiltinFunction { .. } => {
                        self.bind_constant(param.clone(), value);
                    }
                }
            }
            self.compile_expr(&lambda.body)
        })();
        match result {
            Ok(value) => {
                if self.native_value_captures_current_scope(value)
                    || self.queued_threads_capture_current_scope()
                {
                    self.pop_scope_preserving_allocations();
                } else {
                    self.pop_scope();
                }
                Ok(value)
            }
            Err(error) => {
                self.pop_scope();
                Err(error)
            }
        }
    }

    fn compile_lambda_body_with_static_arguments_preserving_effects(
        &mut self,
        params: &[String],
        body: &Expr,
        arguments: Vec<StaticValue>,
        span: Span,
        feature: &str,
    ) -> Result<StaticValue, Diagnostic> {
        if params.len() != arguments.len() {
            return Err(unsupported(span, feature));
        }
        let bindings = params
            .iter()
            .map(String::as_str)
            .zip(arguments.iter().cloned())
            .collect::<Vec<_>>();
        let thread_aliases = self.current_thread_aliases_with_static_bindings(&bindings);
        if !expr_contains_thread_call(body, &thread_aliases)
            && static_expr_is_pure(body)
            && let Some(value) =
                self.static_value_from_expr_with_bindings_preserving_static_scopes(body, &bindings)
        {
            return Ok(value);
        }

        self.push_scope();
        let result = (|| {
            for (param, value) in params.iter().zip(arguments) {
                self.bind_static_runtime_value(param.clone(), value);
            }
            self.compile_expr(body)?;
            self.static_result_after_effectful_eval(body, &bindings)
                .ok_or_else(|| unsupported(span, feature))
        })();
        self.pop_scope();
        result
    }

    fn static_result_after_effectful_eval(
        &mut self,
        expr: &Expr,
        bindings: &[(&str, StaticValue)],
    ) -> Option<StaticValue> {
        let result_expr = match expr {
            Expr::Block { expressions, .. } => {
                let Some(last) = expressions.last() else {
                    return Some(StaticValue::Unit);
                };
                last
            }
            Expr::Cleanup { body, .. } => body,
            _ => expr,
        };
        if !static_expr_is_pure(result_expr) {
            return None;
        }
        self.static_value_from_expr_with_bindings_preserving_static_scopes(result_expr, bindings)
    }

    fn static_apply_lambda_value(
        &mut self,
        receiver: Option<StaticValue>,
        label: LambdaLabel,
        arguments: Vec<StaticValue>,
    ) -> Option<StaticValue> {
        let lambda = self.static_lambdas.get(label.0)?.clone();
        if lambda.contains_thread_call
            || !static_expr_is_pure(&lambda.body)
            || self.lambda_uses_runtime_captures(&lambda)
        {
            return None;
        }
        let receives_receiver = if lambda.params.len() == arguments.len() + 1 {
            true
        } else if lambda.params.len() == arguments.len() {
            false
        } else {
            return None;
        };

        let receiver = if receives_receiver { receiver } else { None };
        let mut bindings = Vec::with_capacity(lambda.params.len());
        if let Some(receiver) = receiver {
            bindings.push((lambda.params[0].as_str(), receiver));
        }
        let params = lambda.params.iter().skip(usize::from(receives_receiver));
        for (param, value) in params.zip(arguments) {
            bindings.push((param.as_str(), value));
        }

        self.push_scope();
        for (name, value) in lambda.captures.clone() {
            self.bind_static_value(name, value);
        }
        let result = self.static_value_from_expr_with_bindings(&lambda.body, &bindings);
        self.pop_scope();
        result
    }

    fn static_direct_bind_value(&mut self, list: &Expr, mapper: &Expr) -> Option<StaticValue> {
        let element = self
            .static_list_values_from_expr(list)?
            .into_iter()
            .next()?;
        self.static_apply_callable_value(mapper, vec![element])
    }

    fn static_list_values_from_expr(&mut self, expr: &Expr) -> Option<Vec<StaticValue>> {
        let value = self.static_value_from_pure_expr(expr)?;
        self.static_list_values_from_value(&value)
    }

    fn static_list_values_from_value(&self, value: &StaticValue) -> Option<Vec<StaticValue>> {
        match value {
            StaticValue::StaticIntList { label, len } => Some(
                self.asm
                    .i64s_for_label(*label, *len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect(),
            ),
            StaticValue::StaticList { label } => self
                .static_lists
                .get(label.0)
                .map(|list| list.elements.clone()),
            _ => None,
        }
    }

    fn static_list_value_from_elements(&mut self, elements: Vec<StaticValue>) -> StaticValue {
        if let Some(ints) = elements
            .iter()
            .map(|element| match element {
                StaticValue::Int(value) => Some(*value),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
        {
            let label = self.asm.data_label_with_i64s(&ints);
            StaticValue::StaticIntList {
                label,
                len: ints.len(),
            }
        } else {
            let label = self.intern_static_list(elements);
            StaticValue::StaticList { label }
        }
    }

    fn static_call_value_by_name(&mut self, name: &str, arguments: &[Expr]) -> Option<StaticValue> {
        let canonical_name = self.canonical_builtin_name(name);
        let name = canonical_name.as_str();
        if let Some(function) = self.functions.get(name).cloned() {
            let values = arguments
                .iter()
                .map(|argument| self.static_value_from_pure_expr(argument))
                .collect::<Option<Vec<_>>>()?;
            if let Some(value) = self.static_function_call_value_from_values(&function, &values) {
                return Some(value);
            }
        }
        match name {
            "map" if arguments.len() == 2 => self.static_direct_map_value(arguments),
            "bind" if arguments.len() == 2 => {
                self.static_direct_bind_value(&arguments[0], &arguments[1])
            }
            "unit" if arguments.len() == 1 => {
                let value = self.static_value_from_pure_expr(&arguments[0])?;
                let label = self.intern_static_list(vec![value]);
                Some(StaticValue::StaticList { label })
            }
            "double" | "sqrt" | "int" | "floor" | "ceil" | "abs" if arguments.len() == 1 => {
                self.static_numeric_call_value(name, arguments)
            }
            "toString" if arguments.len() == 1 => {
                let value = self.static_value_from_pure_expr(&arguments[0])?;
                let text = self.static_value_display_string(&value);
                Some(self.static_string_value(text))
            }
            "size" | "Map#size" | "Set#size" if arguments.len() == 1 => {
                let value = self.static_value_from_pure_expr(&arguments[0])?;
                let len = match (name, value) {
                    ("size", StaticValue::StaticIntList { len, .. }) => len,
                    ("size", StaticValue::StaticList { label }) => {
                        self.static_lists.get(label.0)?.elements.len()
                    }
                    ("size", StaticValue::StaticMap { label })
                    | ("Map#size", StaticValue::StaticMap { label }) => {
                        self.static_maps.get(label.0)?.entries.len()
                    }
                    ("size", StaticValue::StaticSet { label })
                    | ("Set#size", StaticValue::StaticSet { label }) => {
                        self.static_sets.get(label.0)?.elements.len()
                    }
                    _ => return None,
                };
                Some(StaticValue::Int(len as i64))
            }
            "isEmpty" | "Map#isEmpty" | "Set#isEmpty" if arguments.len() == 1 => {
                let value = self.static_value_from_pure_expr(&arguments[0])?;
                let is_empty = match (name, value) {
                    ("isEmpty", StaticValue::StaticIntList { len, .. }) => len == 0,
                    ("isEmpty", StaticValue::StaticList { label }) => {
                        self.static_lists.get(label.0)?.elements.is_empty()
                    }
                    ("isEmpty", StaticValue::StaticMap { label })
                    | ("Map#isEmpty", StaticValue::StaticMap { label }) => {
                        self.static_maps.get(label.0)?.entries.is_empty()
                    }
                    ("isEmpty", StaticValue::StaticSet { label })
                    | ("Set#isEmpty", StaticValue::StaticSet { label }) => {
                        self.static_sets.get(label.0)?.elements.is_empty()
                    }
                    _ => return None,
                };
                Some(StaticValue::Bool(is_empty))
            }
            "isEmptyString" if arguments.len() == 1 => {
                let value = self.static_value_from_pure_expr(&arguments[0])?;
                Some(StaticValue::Bool(
                    self.static_string_from_value(&value)?.is_empty(),
                ))
            }
            "head" if arguments.len() == 1 => {
                match self.static_value_from_pure_expr(&arguments[0])? {
                    StaticValue::StaticIntList { label, len } => {
                        (len > 0).then(|| StaticValue::Int(self.asm.i64s_for_label(label, len)[0]))
                    }
                    StaticValue::StaticList { label } => {
                        self.static_lists.get(label.0)?.elements.first().cloned()
                    }
                    _ => None,
                }
            }
            "tail" if arguments.len() == 1 => {
                match self.static_value_from_pure_expr(&arguments[0])? {
                    StaticValue::StaticIntList { label, len } => {
                        let values = self.asm.i64s_for_label(label, len);
                        let tail = values.get(1..).unwrap_or_default();
                        let label = self.asm.data_label_with_i64s(tail);
                        Some(StaticValue::StaticIntList {
                            label,
                            len: tail.len(),
                        })
                    }
                    StaticValue::StaticList { label } => {
                        let elements = self
                            .static_lists
                            .get(label.0)?
                            .elements
                            .iter()
                            .skip(1)
                            .cloned()
                            .collect();
                        let label = self.intern_static_list(elements);
                        Some(StaticValue::StaticList { label })
                    }
                    _ => None,
                }
            }
            "join" if arguments.len() == 2 => {
                let list = self.static_value_from_pure_expr(&arguments[0])?;
                let delimiter_value = self.static_value_from_pure_expr(&arguments[1])?;
                let delimiter = self.static_string_from_value(&delimiter_value)?;
                let StaticValue::StaticList { label } = list else {
                    return None;
                };
                let elements = self.static_lists.get(label.0)?.elements.clone();
                let parts = elements
                    .iter()
                    .map(|value| self.static_string_from_value(value))
                    .collect::<Option<Vec<_>>>()?;
                Some(self.static_string_value(parts.join(&delimiter)))
            }
            "contains" | "Set#contains" if arguments.len() == 2 => {
                let value = self.static_value_from_pure_expr(&arguments[0])?;
                let needle = self.static_value_from_pure_expr(&arguments[1])?;
                match &value {
                    StaticValue::StaticString { .. } if name == "contains" => {
                        let input = self.static_string_from_value(&value)?;
                        let needle = self.static_string_from_value(&needle)?;
                        Some(StaticValue::Bool(input.contains(&needle)))
                    }
                    StaticValue::StaticIntList { .. } | StaticValue::StaticList { .. }
                        if name == "contains" =>
                    {
                        let elements = self.static_list_values_from_value(&value)?;
                        Some(StaticValue::Bool(elements.iter().any(|element| {
                            self.static_value_equal_user(element, &needle)
                        })))
                    }
                    StaticValue::StaticSet { label } => {
                        let elements = self.static_sets.get(label.0)?.elements.clone();
                        Some(StaticValue::Bool(elements.iter().any(|element| {
                            self.static_value_equal_user(element, &needle)
                        })))
                    }
                    _ => None,
                }
            }
            "Map#containsKey" | "containsKey" if arguments.len() == 2 => {
                let map = self.static_value_from_pure_expr(&arguments[0])?;
                let key = self.static_value_from_pure_expr(&arguments[1])?;
                let StaticValue::StaticMap { label } = map else {
                    return None;
                };
                let entries = self.static_maps.get(label.0)?.entries.clone();
                Some(StaticValue::Bool(entries.iter().any(|(entry_key, _)| {
                    self.static_value_equal_user(entry_key, &key)
                })))
            }
            "Map#containsValue" | "containsValue" if arguments.len() == 2 => {
                let map = self.static_value_from_pure_expr(&arguments[0])?;
                let value = self.static_value_from_pure_expr(&arguments[1])?;
                let StaticValue::StaticMap { label } = map else {
                    return None;
                };
                let entries = self.static_maps.get(label.0)?.entries.clone();
                Some(StaticValue::Bool(entries.iter().any(|(_, entry_value)| {
                    self.static_value_equal_user(entry_value, &value)
                })))
            }
            "Map#get" | "get" if arguments.len() == 2 => {
                let map = self.static_value_from_pure_expr(&arguments[0])?;
                let key = self.static_value_from_pure_expr(&arguments[1])?;
                let StaticValue::StaticMap { label } = map else {
                    return None;
                };
                let entries = self.static_maps.get(label.0)?.entries.clone();
                Some(
                    entries
                        .iter()
                        .find(|(entry_key, _)| self.static_value_equal_user(entry_key, &key))
                        .map(|(_, value)| value.clone())
                        .unwrap_or(StaticValue::Null),
                )
            }
            _ => self.static_instance_method_call_value(name, arguments),
        }
    }

    fn static_call_value_by_name_with_values(
        &mut self,
        name: &str,
        arguments: &[StaticValue],
    ) -> Option<StaticValue> {
        let canonical_name = self.canonical_builtin_name(name);
        let name = canonical_name.as_str();
        if let Some(function) = self.functions.get(name).cloned()
            && let Some(value) = self.static_function_call_value_from_values(&function, arguments)
        {
            return Some(value);
        }
        if let Some(value) = self.static_string_helper_call_value_from_values(name, arguments) {
            return Some(value);
        }
        match name {
            "unit" if arguments.len() == 1 => {
                let label = self.intern_static_list(vec![arguments[0].clone()]);
                Some(StaticValue::StaticList { label })
            }
            "double" | "sqrt" | "int" | "floor" | "ceil" | "abs" if arguments.len() == 1 => {
                self.static_numeric_call_value_from_value(name, arguments[0].clone())
            }
            "toString" if arguments.len() == 1 => {
                let text = self.static_value_display_string(&arguments[0]);
                Some(self.static_string_value(text))
            }
            "join" if arguments.len() == 2 => {
                let delimiter = self.static_string_from_value(&arguments[1])?;
                let parts = self
                    .static_list_values_from_value(&arguments[0])?
                    .iter()
                    .map(|value| self.static_string_from_value(value))
                    .collect::<Option<Vec<_>>>()?;
                Some(self.static_string_value(parts.join(&delimiter)))
            }
            "size" | "Map#size" | "Set#size" if arguments.len() == 1 => {
                let len = match (name, &arguments[0]) {
                    ("size", StaticValue::StaticIntList { len, .. }) => *len,
                    ("size", StaticValue::StaticList { label }) => {
                        self.static_lists.get(label.0)?.elements.len()
                    }
                    ("size", StaticValue::StaticMap { label })
                    | ("Map#size", StaticValue::StaticMap { label }) => {
                        self.static_maps.get(label.0)?.entries.len()
                    }
                    ("size", StaticValue::StaticSet { label })
                    | ("Set#size", StaticValue::StaticSet { label }) => {
                        self.static_sets.get(label.0)?.elements.len()
                    }
                    _ => return None,
                };
                Some(StaticValue::Int(len as i64))
            }
            "isEmpty" | "Map#isEmpty" | "Set#isEmpty" if arguments.len() == 1 => {
                let is_empty = match (name, &arguments[0]) {
                    ("isEmpty", StaticValue::StaticIntList { len, .. }) => *len == 0,
                    ("isEmpty", StaticValue::StaticList { label }) => {
                        self.static_lists.get(label.0)?.elements.is_empty()
                    }
                    ("isEmpty", StaticValue::StaticMap { label })
                    | ("Map#isEmpty", StaticValue::StaticMap { label }) => {
                        self.static_maps.get(label.0)?.entries.is_empty()
                    }
                    ("isEmpty", StaticValue::StaticSet { label })
                    | ("Set#isEmpty", StaticValue::StaticSet { label }) => {
                        self.static_sets.get(label.0)?.elements.is_empty()
                    }
                    _ => return None,
                };
                Some(StaticValue::Bool(is_empty))
            }
            "head" if arguments.len() == 1 => self
                .static_list_values_from_value(&arguments[0])?
                .into_iter()
                .next(),
            "tail" if arguments.len() == 1 => {
                let tail = self
                    .static_list_values_from_value(&arguments[0])?
                    .into_iter()
                    .skip(1)
                    .collect::<Vec<_>>();
                Some(self.static_list_value_from_elements(tail))
            }
            "contains" | "Set#contains" if arguments.len() == 2 => match &arguments[0] {
                StaticValue::StaticString { .. } if name == "contains" => {
                    let input = self.static_string_from_value(&arguments[0])?;
                    let needle = self.static_string_from_value(&arguments[1])?;
                    Some(StaticValue::Bool(input.contains(&needle)))
                }
                StaticValue::StaticIntList { .. } | StaticValue::StaticList { .. }
                    if name == "contains" =>
                {
                    let elements = self.static_list_values_from_value(&arguments[0])?;
                    Some(StaticValue::Bool(elements.iter().any(|element| {
                        self.static_value_equal_user(element, &arguments[1])
                    })))
                }
                StaticValue::StaticSet { label } => {
                    let elements = self.static_sets.get(label.0)?.elements.clone();
                    Some(StaticValue::Bool(elements.iter().any(|element| {
                        self.static_value_equal_user(element, &arguments[1])
                    })))
                }
                _ => None,
            },
            "Map#containsKey" | "containsKey" if arguments.len() == 2 => {
                let StaticValue::StaticMap { label } = &arguments[0] else {
                    return None;
                };
                let entries = self.static_maps.get(label.0)?.entries.clone();
                Some(StaticValue::Bool(entries.iter().any(|(entry_key, _)| {
                    self.static_value_equal_user(entry_key, &arguments[1])
                })))
            }
            "Map#containsValue" | "containsValue" if arguments.len() == 2 => {
                let StaticValue::StaticMap { label } = &arguments[0] else {
                    return None;
                };
                let entries = self.static_maps.get(label.0)?.entries.clone();
                Some(StaticValue::Bool(entries.iter().any(|(_, entry_value)| {
                    self.static_value_equal_user(entry_value, &arguments[1])
                })))
            }
            "Map#get" | "get" if arguments.len() == 2 => {
                let StaticValue::StaticMap { label } = &arguments[0] else {
                    return None;
                };
                let entries = self.static_maps.get(label.0)?.entries.clone();
                Some(
                    entries
                        .iter()
                        .find(|(entry_key, _)| {
                            self.static_value_equal_user(entry_key, &arguments[1])
                        })
                        .map(|(_, value)| value.clone())
                        .unwrap_or(StaticValue::Null),
                )
            }
            "FileInput#readAll" | "FileInput#all" if arguments.len() == 1 => {
                let path = self.static_string_from_value(&arguments[0])?;
                let content = self
                    .static_file_content(&path, Span::new(0, 0), name)
                    .ok()?;
                Some(self.static_string_value(content))
            }
            "FileInput#readLines" | "FileInput#lines" if arguments.len() == 1 => {
                let path = self.static_string_from_value(&arguments[0])?;
                let content = self
                    .static_file_content(&path, Span::new(0, 0), name)
                    .ok()?;
                let elements = content
                    .lines()
                    .map(|line| self.static_string_value(line.to_string()))
                    .collect::<Vec<_>>();
                let label = self.intern_static_list(elements);
                Some(StaticValue::StaticList { label })
            }
            _ => self.static_instance_method_call_value_from_values(name, arguments),
        }
    }

    fn static_string_helper_call_value_from_values(
        &mut self,
        name: &str,
        arguments: &[StaticValue],
    ) -> Option<StaticValue> {
        match name {
            "toString" if arguments.len() == 1 => {
                let text = self.static_value_display_string(&arguments[0]);
                Some(self.static_string_value(text))
            }
            "substring" if arguments.len() == 3 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let start = static_non_negative_int_from_value(&arguments[1])?;
                let end = static_non_negative_int_from_value(&arguments[2])?;
                let chars = input.chars().collect::<Vec<_>>();
                let start = start.min(chars.len());
                let end = end.min(chars.len()).max(start);
                Some(self.static_string_value(chars[start..end].iter().collect()))
            }
            "at" if arguments.len() == 2 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let index = static_non_negative_int_from_value(&arguments[1])?;
                let chars = input.chars().collect::<Vec<_>>();
                let value = chars
                    .get(index.min(chars.len()))
                    .copied()
                    .unwrap_or_default();
                Some(self.static_string_value(if value == '\0' {
                    String::new()
                } else {
                    value.to_string()
                }))
            }
            "matches" if arguments.len() == 2 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let pattern = self.static_string_from_value(&arguments[1])?;
                Some(StaticValue::Bool(simple_regex_is_match(&input, &pattern)))
            }
            "split" if arguments.len() == 2 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let delimiter = self.static_string_from_value(&arguments[1])?;
                let elements = if delimiter.is_empty() {
                    input
                        .chars()
                        .map(|ch| self.static_string_value(ch.to_string()))
                        .collect::<Vec<_>>()
                } else {
                    input
                        .split(&delimiter)
                        .map(|part| self.static_string_value(part.to_string()))
                        .collect::<Vec<_>>()
                };
                let label = self.intern_static_list(elements);
                Some(StaticValue::StaticList { label })
            }
            "trim" | "trimLeft" | "trimRight" | "toLowerCase" | "toUpperCase" | "reverse"
                if arguments.len() == 1 =>
            {
                let input = self.static_string_from_value(&arguments[0])?;
                let output = match name {
                    "trim" => input.trim().to_string(),
                    "trimLeft" => input.trim_start().to_string(),
                    "trimRight" => input.trim_end().to_string(),
                    "toLowerCase" => input.to_lowercase(),
                    "toUpperCase" => input.to_uppercase(),
                    "reverse" => input.chars().rev().collect(),
                    _ => unreachable!("string unary helper matched above"),
                };
                Some(self.static_string_value(output))
            }
            "replace" | "replaceAll" if arguments.len() == 3 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let from = self.static_string_from_value(&arguments[1])?;
                let to = self.static_string_from_value(&arguments[2])?;
                let output = if name == "replace" {
                    input.replacen(&from, &to, 1)
                } else {
                    simple_regex_replace_all(&input, &from, &to)
                };
                Some(self.static_string_value(output))
            }
            "startsWith" | "endsWith" | "contains" if arguments.len() == 2 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let needle = self.static_string_from_value(&arguments[1])?;
                let value = match name {
                    "startsWith" => input.starts_with(&needle),
                    "endsWith" => input.ends_with(&needle),
                    "contains" => input.contains(&needle),
                    _ => unreachable!("string predicate helper matched above"),
                };
                Some(StaticValue::Bool(value))
            }
            "isEmptyString" if arguments.len() == 1 => {
                let input = self.static_string_from_value(&arguments[0])?;
                Some(StaticValue::Bool(input.is_empty()))
            }
            "indexOf" | "lastIndexOf" if arguments.len() == 2 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let needle = self.static_string_from_value(&arguments[1])?;
                let index = if name == "indexOf" {
                    input.find(&needle)
                } else {
                    input.rfind(&needle)
                }
                .map(|index| index as i64)
                .unwrap_or(-1);
                Some(StaticValue::Int(index))
            }
            "length" if arguments.len() == 1 => {
                let input = self.static_string_from_value(&arguments[0])?;
                Some(StaticValue::Int(input.chars().count() as i64))
            }
            "repeat" if arguments.len() == 2 => {
                let input = self.static_string_from_value(&arguments[0])?;
                let count = static_non_negative_int_from_value(&arguments[1])?;
                Some(self.static_string_value(input.repeat(count)))
            }
            _ => None,
        }
    }

    fn static_numeric_call_value(&mut self, name: &str, arguments: &[Expr]) -> Option<StaticValue> {
        if arguments.len() != 1 {
            return None;
        }
        let value = self.static_value_from_pure_expr(&arguments[0])?;
        self.static_numeric_call_value_from_value(name, value)
    }

    fn preview_static_value_after_effectful_eval(&mut self, expr: &Expr) -> Option<StaticValue> {
        let before_static_scopes = self.static_scopes.clone();
        let value = self
            .static_result_after_effectful_eval(expr, &[])
            .or_else(|| self.static_value_from_expr(expr));
        self.static_scopes = before_static_scopes;
        value
    }

    fn static_numeric_call_value_from_value(
        &mut self,
        name: &str,
        value: StaticValue,
    ) -> Option<StaticValue> {
        match name {
            "double" => Some(StaticValue::Double(static_value_as_f64(&value)?.to_bits())),
            "sqrt" => Some(StaticValue::Double(
                static_value_as_f64(&value)?.sqrt().to_bits(),
            )),
            "int" | "floor" => match value {
                StaticValue::Int(value) => Some(StaticValue::Int(value)),
                StaticValue::Float(bits) => Some(StaticValue::Int(f32::from_bits(bits) as i64)),
                StaticValue::Double(bits) => Some(StaticValue::Int(f64::from_bits(bits) as i64)),
                _ => None,
            },
            "ceil" => match value {
                StaticValue::Int(value) => Some(StaticValue::Int(value)),
                StaticValue::Float(bits) => {
                    Some(StaticValue::Int(f32::from_bits(bits).ceil() as i64))
                }
                StaticValue::Double(bits) => {
                    Some(StaticValue::Int(f64::from_bits(bits).ceil() as i64))
                }
                _ => None,
            },
            "abs" => match value {
                StaticValue::Int(value) => value.checked_abs().map(StaticValue::Int),
                StaticValue::Float(bits) => {
                    Some(StaticValue::Float(f32::from_bits(bits).abs().to_bits()))
                }
                StaticValue::Double(bits) => {
                    Some(StaticValue::Double(f64::from_bits(bits).abs().to_bits()))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn static_string_from_value(&self, value: &StaticValue) -> Option<String> {
        let StaticValue::StaticString { label, len } = value else {
            return None;
        };
        String::from_utf8(self.asm.data_bytes_for_label(*label, *len).to_vec()).ok()
    }

    fn static_value_from_expr_with_bindings(
        &mut self,
        expr: &Expr,
        bindings: &[(&str, StaticValue)],
    ) -> Option<StaticValue> {
        match expr {
            Expr::String { value, .. } if value.contains("#{") => self
                .static_interpolated_string_value_with_bindings(value, bindings)
                .map(|value| self.static_string_value(value)),
            Expr::Identifier { name, .. } => bindings
                .iter()
                .find_map(|(binding, value)| (*binding == name).then(|| value.clone()))
                .or_else(|| self.static_value_from_expr(expr)),
            Expr::FieldAccess { target, field, .. } => {
                let StaticValue::StaticRecord { label } =
                    self.static_value_from_expr_with_bindings(target, bindings)?
                else {
                    return None;
                };
                self.static_record_field(label, field)
            }
            Expr::Lambda { params, body, .. } => {
                let mut captures = self.current_static_captures();
                for (name, value) in bindings {
                    captures.insert((*name).to_string(), value.clone());
                }
                let thread_aliases = self.current_thread_aliases_with_static_bindings(bindings);
                let label = self.intern_static_lambda(
                    params.clone(),
                    body.as_ref().clone(),
                    captures,
                    self.current_runtime_captures(),
                    expr_contains_thread_call(body, &thread_aliases),
                );
                Some(StaticValue::StaticLambda { label })
            }
            Expr::Block { expressions, .. } => {
                let mut last = StaticValue::Unit;
                for expression in expressions {
                    last = self.static_value_from_expr_with_bindings(expression, bindings)?;
                }
                Some(last)
            }
            Expr::Unary {
                op: UnaryOp::Plus,
                expr,
                ..
            } => self.static_value_from_expr_with_bindings(expr, bindings),
            Expr::Unary {
                op: UnaryOp::Minus,
                expr,
                ..
            } => match self.static_value_from_expr_with_bindings(expr, bindings)? {
                StaticValue::Int(value) => value.checked_neg().map(StaticValue::Int),
                StaticValue::Float(bits) => {
                    Some(StaticValue::Float((-f32::from_bits(bits)).to_bits()))
                }
                StaticValue::Double(bits) => {
                    Some(StaticValue::Double((-f64::from_bits(bits)).to_bits()))
                }
                _ => None,
            },
            Expr::Binary { lhs, op, rhs, .. } => {
                if matches!(op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
                    let StaticValue::Bool(lhs) =
                        self.static_value_from_expr_with_bindings(lhs, bindings)?
                    else {
                        return None;
                    };
                    match (op, lhs) {
                        (BinaryOp::LogicalAnd, false) => return Some(StaticValue::Bool(false)),
                        (BinaryOp::LogicalOr, true) => return Some(StaticValue::Bool(true)),
                        _ => {}
                    }
                    let StaticValue::Bool(rhs) =
                        self.static_value_from_expr_with_bindings(rhs, bindings)?
                    else {
                        return None;
                    };
                    return Some(StaticValue::Bool(rhs));
                }
                let lhs = self.static_value_from_expr_with_bindings(lhs, bindings)?;
                let rhs = self.static_value_from_expr_with_bindings(rhs, bindings)?;
                if *op == BinaryOp::Add
                    && (matches!(lhs, StaticValue::StaticString { .. })
                        || matches!(rhs, StaticValue::StaticString { .. }))
                {
                    let lhs = self.static_value_display_string(&lhs);
                    let rhs = self.static_value_display_string(&rhs);
                    return Some(self.static_string_value(format!("{lhs}{rhs}")));
                }
                match (lhs, op, rhs) {
                    (StaticValue::Int(lhs), BinaryOp::Add, StaticValue::Int(rhs)) => {
                        lhs.checked_add(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::Subtract, StaticValue::Int(rhs)) => {
                        lhs.checked_sub(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::Multiply, StaticValue::Int(rhs)) => {
                        lhs.checked_mul(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::Divide, StaticValue::Int(rhs))
                        if rhs != 0 =>
                    {
                        lhs.checked_div(rhs).map(StaticValue::Int)
                    }
                    (StaticValue::Int(lhs), BinaryOp::BitAnd, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Int(lhs & rhs))
                    }
                    (StaticValue::Int(lhs), BinaryOp::BitOr, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Int(lhs | rhs))
                    }
                    (StaticValue::Int(lhs), BinaryOp::BitXor, StaticValue::Int(rhs)) => {
                        Some(StaticValue::Int(lhs ^ rhs))
                    }
                    (lhs, op, rhs)
                        if matches!(
                            op,
                            BinaryOp::Add
                                | BinaryOp::Subtract
                                | BinaryOp::Multiply
                                | BinaryOp::Divide
                        ) =>
                    {
                        static_numeric_binary_value(*op, &lhs, &rhs)
                    }
                    (lhs, BinaryOp::Equal, rhs) => {
                        Some(StaticValue::Bool(self.static_value_equal_user(&lhs, &rhs)))
                    }
                    (lhs, BinaryOp::NotEqual, rhs) => {
                        Some(StaticValue::Bool(!self.static_value_equal_user(&lhs, &rhs)))
                    }
                    (StaticValue::Bool(lhs), BinaryOp::LogicalAnd, StaticValue::Bool(rhs)) => {
                        Some(StaticValue::Bool(lhs && rhs))
                    }
                    (StaticValue::Bool(lhs), BinaryOp::LogicalOr, StaticValue::Bool(rhs)) => {
                        Some(StaticValue::Bool(lhs || rhs))
                    }
                    (lhs, BinaryOp::Less, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? < static_value_as_f64(&rhs)?,
                        ))
                    }
                    (lhs, BinaryOp::LessEqual, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? <= static_value_as_f64(&rhs)?,
                        ))
                    }
                    (lhs, BinaryOp::Greater, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? > static_value_as_f64(&rhs)?,
                        ))
                    }
                    (lhs, BinaryOp::GreaterEqual, rhs)
                        if static_value_as_f64(&lhs).is_some()
                            && static_value_as_f64(&rhs).is_some() =>
                    {
                        Some(StaticValue::Bool(
                            static_value_as_f64(&lhs)? >= static_value_as_f64(&rhs)?,
                        ))
                    }
                    _ => None,
                }
            }
            Expr::ListLiteral { elements, .. } => {
                let elements = elements
                    .iter()
                    .map(|element| self.static_value_from_expr_with_bindings(element, bindings))
                    .collect::<Option<Vec<_>>>()?;
                Some(self.static_list_value_from_elements(elements))
            }
            Expr::MapLiteral { entries, .. } => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| {
                        let key = self.static_value_from_expr_with_bindings(key, bindings)?;
                        let value = self.static_value_from_expr_with_bindings(value, bindings)?;
                        Some((key, value))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let label = self.intern_static_map(entries);
                Some(StaticValue::StaticMap { label })
            }
            Expr::SetLiteral { elements, .. } => {
                let mut values = Vec::new();
                for element in elements {
                    let value = self.static_value_from_expr_with_bindings(element, bindings)?;
                    if !values
                        .iter()
                        .any(|existing| self.static_value_equal_user(existing, &value))
                    {
                        values.push(value);
                    }
                }
                let label = self.intern_static_set(values);
                Some(StaticValue::StaticSet { label })
            }
            Expr::RecordLiteral { fields, .. } => {
                let fields = fields
                    .iter()
                    .map(|(field, value)| {
                        self.static_value_from_expr_with_bindings(value, bindings)
                            .map(|value| (field.clone(), value))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let label = self.intern_static_record(String::new(), fields);
                Some(StaticValue::StaticRecord { label })
            }
            Expr::RecordConstructor {
                name, arguments, ..
            } => {
                let fields = self.record_schemas.get(name)?.clone();
                if fields.len() != arguments.len() {
                    return None;
                }
                let fields = fields
                    .into_iter()
                    .zip(arguments.iter())
                    .map(|(field, value)| {
                        self.static_value_from_expr_with_bindings(value, bindings)
                            .map(|value| (field, value))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let label = self.intern_static_record(name.clone(), fields);
                Some(StaticValue::StaticRecord { label })
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                if let Expr::FieldAccess { target, field, .. } = callee.as_ref() {
                    let receiver = self.static_value_from_expr_with_bindings(target, bindings)?;
                    if let StaticValue::StaticRecord { label } = receiver.clone()
                        && let Some(StaticValue::StaticLambda { label }) =
                            self.static_record_field(label, field)
                    {
                        let argument_values = arguments
                            .iter()
                            .map(|argument| {
                                self.static_value_from_expr_with_bindings(argument, bindings)
                            })
                            .collect::<Option<Vec<_>>>()?;
                        return self.static_apply_lambda_value(
                            Some(receiver),
                            label,
                            argument_values,
                        );
                    }
                }
                if let Expr::Call {
                    callee: inner_callee,
                    arguments: initial_arguments,
                    ..
                } = callee.as_ref()
                    && let Expr::Identifier { name, .. } = inner_callee.as_ref()
                    && initial_arguments.len() == 1
                    && arguments.len() == 1
                    && matches!(name.as_str(), "map" | "bind")
                {
                    let list =
                        self.static_value_from_expr_with_bindings(&initial_arguments[0], bindings)?;
                    if name == "map" {
                        let mapped = self
                            .static_list_values_from_value(&list)?
                            .into_iter()
                            .map(|element| {
                                self.static_apply_callable_value(&arguments[0], vec![element])
                            })
                            .collect::<Option<Vec<_>>>()?;
                        return Some(self.static_list_value_from_elements(mapped));
                    }
                    let element = self
                        .static_list_values_from_value(&list)?
                        .into_iter()
                        .next()?;
                    return self.static_apply_callable_value(&arguments[0], vec![element]);
                }
                if let Expr::Identifier { name, .. } = callee.as_ref() {
                    let argument_values = arguments
                        .iter()
                        .map(|argument| {
                            self.static_value_from_expr_with_bindings(argument, bindings)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    if let Some(StaticValue::StaticLambda { label }) =
                        self.static_value_from_expr_with_bindings(callee, bindings)
                        && let Some(value) =
                            self.static_apply_lambda_value(None, label, argument_values.clone())
                    {
                        return Some(value);
                    }
                    if let Some(value) =
                        self.static_call_value_by_name_with_values(name, &argument_values)
                    {
                        return Some(value);
                    }
                }
                if let Expr::Call {
                    callee: inner_callee,
                    arguments: head_arguments,
                    ..
                } = callee.as_ref()
                    && let Expr::Identifier { name, .. } = inner_callee.as_ref()
                    && name == "cons"
                    && head_arguments.len() == 1
                    && arguments.len() == 1
                {
                    let head =
                        self.static_value_from_expr_with_bindings(&head_arguments[0], bindings)?;
                    let tail =
                        self.static_value_from_expr_with_bindings(&arguments[0], bindings)?;
                    return self.static_cons_value(head, tail);
                }
                self.static_value_from_expr(expr)
            }
            _ => self.static_value_from_expr(expr),
        }
    }

    fn static_value_from_expr_with_bindings_preserving_static_scopes(
        &mut self,
        expr: &Expr,
        bindings: &[(&str, StaticValue)],
    ) -> Option<StaticValue> {
        let saved_static_scopes = self.static_scopes.clone();
        let result = self.static_value_from_expr_with_bindings(expr, bindings);
        self.static_scopes = saved_static_scopes;
        result
    }

    fn static_value_from_native(&self, value: NativeValue) -> Option<StaticValue> {
        match value {
            NativeValue::Null => Some(StaticValue::Null),
            NativeValue::Unit => Some(StaticValue::Unit),
            NativeValue::StaticFloat { bits } => Some(StaticValue::Float(bits)),
            NativeValue::StaticDouble { bits } => Some(StaticValue::Double(bits)),
            NativeValue::StaticString { label, len } => {
                Some(StaticValue::StaticString { label, len })
            }
            NativeValue::StaticIntList { label, len } => {
                Some(StaticValue::StaticIntList { label, len })
            }
            NativeValue::StaticList { label } => Some(StaticValue::StaticList { label }),
            NativeValue::StaticRecord { label } => Some(StaticValue::StaticRecord { label }),
            NativeValue::StaticMap { label } => Some(StaticValue::StaticMap { label }),
            NativeValue::StaticSet { label } => Some(StaticValue::StaticSet { label }),
            NativeValue::StaticLambda { label } => Some(StaticValue::StaticLambda { label }),
            NativeValue::BuiltinFunction { label } => Some(StaticValue::BuiltinFunction { label }),
            NativeValue::Int
            | NativeValue::Bool
            | NativeValue::RuntimeString { .. }
            | NativeValue::RuntimeLinesList { .. } => None,
        }
    }

    fn emit_static_value(&mut self, value: &StaticValue) -> NativeValue {
        match value {
            StaticValue::Int(value) => {
                self.asm.mov_imm64(Reg::Rax, *value as u64);
                NativeValue::Int
            }
            StaticValue::Bool(value) => {
                self.asm.mov_imm64(Reg::Rax, u64::from(*value));
                NativeValue::Bool
            }
            StaticValue::Float(bits) => NativeValue::StaticFloat { bits: *bits },
            StaticValue::Double(bits) => NativeValue::StaticDouble { bits: *bits },
            StaticValue::Null => NativeValue::Null,
            StaticValue::Unit => NativeValue::Unit,
            StaticValue::StaticString { label, len } => NativeValue::StaticString {
                label: *label,
                len: *len,
            },
            StaticValue::StaticIntList { label, len } => NativeValue::StaticIntList {
                label: *label,
                len: *len,
            },
            StaticValue::StaticList { label } => NativeValue::StaticList { label: *label },
            StaticValue::StaticRecord { label } => NativeValue::StaticRecord { label: *label },
            StaticValue::StaticMap { label } => NativeValue::StaticMap { label: *label },
            StaticValue::StaticSet { label } => NativeValue::StaticSet { label: *label },
            StaticValue::StaticLambda { label } => NativeValue::StaticLambda { label: *label },
            StaticValue::BuiltinFunction { label } => {
                NativeValue::BuiltinFunction { label: *label }
            }
        }
    }

    fn emit_static_string(&mut self, value: String) -> NativeValue {
        let label = self.asm.data_label_with_bytes(value.as_bytes());
        NativeValue::StaticString {
            label,
            len: value.len(),
        }
    }

    fn static_string_value(&mut self, value: String) -> StaticValue {
        let label = self.asm.data_label_with_bytes(value.as_bytes());
        StaticValue::StaticString {
            label,
            len: value.len(),
        }
    }

    fn static_value_from_argument_preserving_effects(
        &mut self,
        expr: &Expr,
        span: Span,
        feature: &str,
    ) -> Result<StaticValue, Diagnostic> {
        if let Some(value) = self.static_value_from_pure_expr(expr) {
            return Ok(value);
        }

        let before_static_scopes = self.static_scopes.clone();
        let compiled = self.compile_expr(expr)?;
        let after_static_scopes = self.static_scopes.clone();
        if let Some(value) = self.static_value_from_native(compiled) {
            return Ok(value);
        }

        self.static_scopes = before_static_scopes;
        let value = self
            .static_result_after_effectful_eval(expr, &[])
            .or_else(|| self.static_value_from_expr(expr));
        self.static_scopes = after_static_scopes;
        value.ok_or_else(|| unsupported(span, feature))
    }

    fn static_values_from_arguments_preserving_effects(
        &mut self,
        exprs: &[Expr],
        span: Span,
        feature: &str,
    ) -> Result<Vec<StaticValue>, Diagnostic> {
        let mut values = Vec::with_capacity(exprs.len());
        for expr in exprs {
            values.push(self.static_value_from_argument_preserving_effects(expr, span, feature)?);
        }
        Ok(values)
    }

    fn static_list_values_from_argument_preserving_effects(
        &mut self,
        expr: &Expr,
        span: Span,
        feature: &str,
    ) -> Result<Vec<StaticValue>, Diagnostic> {
        let value = self.static_value_from_argument_preserving_effects(expr, span, feature)?;
        self.static_list_values_from_value(&value)
            .ok_or_else(|| unsupported(span, feature))
    }

    fn static_string_from_argument_preserving_effects(
        &mut self,
        expr: &Expr,
        span: Span,
        name: &str,
    ) -> Result<String, Diagnostic> {
        let value = self.static_value_from_argument_preserving_effects(
            expr,
            span,
            &format!("native {name} for non-static string"),
        )?;
        self.static_string_from_value(&value)
            .ok_or_else(|| unsupported(span, &format!("native {name} for non-static string")))
    }

    fn compile_runtime_path_argument(
        &mut self,
        expr: &Expr,
        span: Span,
        name: &str,
    ) -> Result<DataLabel, Diagnostic> {
        self.compile_runtime_path_argument_ref(expr, span, name)
            .map(|(label, _)| label)
    }

    fn compile_runtime_path_argument_ref(
        &mut self,
        expr: &Expr,
        span: Span,
        name: &str,
    ) -> Result<(DataLabel, NativeStringRef), Diagnostic> {
        let value = self.compile_expr(expr)?;
        let Some(path) = self.native_string_ref(value) else {
            return Err(unsupported(
                span,
                &format!("native {name} for non-string path"),
            ));
        };
        let label = self.emit_nul_terminated_path_buffer(path, span, name);
        Ok((label, path))
    }

    fn compile_path_argument_to_label(
        &mut self,
        expr: &Expr,
        span: Span,
        name: &str,
    ) -> Result<(DataLabel, Option<String>), Diagnostic> {
        if self.expr_may_yield_runtime_string(expr) {
            return self
                .compile_runtime_path_argument(expr, span, name)
                .map(|label| (label, None));
        }
        let path = self.static_string_from_argument_preserving_effects(expr, span, name)?;
        let label = self.nul_terminated_data_label(&path);
        Ok((label, Some(path)))
    }

    fn static_write_lines_content_from_native(
        &self,
        value: NativeValue,
        span: Span,
        name: &str,
    ) -> Result<String, Diagnostic> {
        let values = self
            .static_value_from_native(value)
            .and_then(|value| self.static_list_values_from_value(&value))
            .ok_or_else(|| unsupported(span, &format!("native {name} for non-static list")))?;
        let lines = values
            .iter()
            .map(|value| {
                self.static_string_from_value(value).ok_or_else(|| {
                    unsupported(span, &format!("native {name} for non-string list element"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(lines.join("\n"))
    }

    fn runtime_write_lines_content_ref(
        &mut self,
        value: NativeValue,
        span: Span,
    ) -> Result<Option<NativeStringRef>, Diagnostic> {
        let NativeValue::RuntimeLinesList { data, len } = value else {
            return Ok(None);
        };
        let delimiter = self.asm.data_label_with_bytes(b"\n");
        let content = self.emit_runtime_lines_join(
            NativeStringRef {
                data,
                len: NativeStringLen::Runtime(len),
            },
            delimiter,
            1,
            span,
        );
        Ok(self.native_string_ref(content))
    }

    fn compile_string_predicate_helper(
        &mut self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        self.expect_static_arity(name, arguments, 2, span)?;
        let input = self.compile_expr(&arguments[0])?;
        let Some(input) = self.native_string_ref(input) else {
            return Err(unsupported(span, &format!("native {name} for non-string")));
        };
        let needle = self.compile_expr(&arguments[1])?;
        let Some(needle) = self.native_string_ref(needle) else {
            return Err(unsupported(span, &format!("native {name} for non-string")));
        };
        match name {
            "startsWith" => self.emit_native_string_starts_with(input, needle),
            "endsWith" => self.emit_native_string_ends_with(input, needle),
            "contains" => self.emit_native_string_contains(input, needle),
            _ => unreachable!("string predicate helper names are checked by caller"),
        }
        Ok(NativeValue::Bool)
    }

    fn static_non_negative_int_argument_preserving_effects(
        &mut self,
        expr: &Expr,
        name: &str,
        span: Span,
    ) -> Result<usize, Diagnostic> {
        match self.static_value_from_argument_preserving_effects(
            expr,
            span,
            &format!("native {name} for non-static integer argument"),
        )? {
            StaticValue::Int(value) if value >= 0 => Ok(value as usize),
            StaticValue::Int(_) => {
                self.emit_runtime_error(
                    span,
                    &format!("{name} expects a non-negative integer index"),
                );
                Ok(0)
            }
            _ => Err(unsupported(
                span,
                &format!("native {name} for non-static integer argument"),
            )),
        }
    }

    fn string_from_data_label(
        &self,
        label: DataLabel,
        len: usize,
        span: Span,
        name: &str,
    ) -> Result<String, Diagnostic> {
        String::from_utf8(self.asm.data_bytes_for_label(label, len).to_vec()).map_err(|error| {
            Diagnostic::compile(
                span,
                format!("native {name} encountered invalid UTF-8 string data: {error}"),
            )
        })
    }

    fn expect_static_arity(
        &self,
        name: &str,
        arguments: &[Expr],
        expected: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if arguments.len() == expected {
            Ok(())
        } else {
            Err(Diagnostic::compile(
                span,
                format!(
                    "{name} expects {expected} arguments but got {}",
                    arguments.len()
                ),
            ))
        }
    }

    fn static_list_elements_from_exprs(&mut self, elements: &[Expr]) -> Option<Vec<StaticValue>> {
        elements
            .iter()
            .map(|element| self.static_value_from_pure_expr(element))
            .collect::<Option<Vec<_>>>()
    }

    fn static_set_elements_from_exprs(&mut self, elements: &[Expr]) -> Option<Vec<StaticValue>> {
        let mut values = Vec::new();
        for element in elements {
            let value = self.static_value_from_pure_expr(element)?;
            if !values
                .iter()
                .any(|existing| self.static_value_equal_user(existing, &value))
            {
                values.push(value);
            }
        }
        Some(values)
    }

    fn static_cons_value(&mut self, head: StaticValue, tail: StaticValue) -> Option<StaticValue> {
        match (head, tail) {
            (StaticValue::Int(head), StaticValue::StaticIntList { label, len }) => {
                let mut values = Vec::with_capacity(len + 1);
                values.push(head);
                values.extend(self.asm.i64s_for_label(label, len));
                let label = self.asm.data_label_with_i64s(&values);
                Some(StaticValue::StaticIntList {
                    label,
                    len: values.len(),
                })
            }
            (head, StaticValue::StaticIntList { label, len }) => {
                let mut elements = Vec::with_capacity(len + 1);
                elements.push(head);
                elements.extend(
                    self.asm
                        .i64s_for_label(label, len)
                        .into_iter()
                        .map(StaticValue::Int),
                );
                let label = self.intern_static_list(elements);
                Some(StaticValue::StaticList { label })
            }
            (head, StaticValue::StaticList { label }) => {
                let mut elements = self
                    .static_lists
                    .get(label.0)
                    .map(|list| list.elements.clone())
                    .unwrap_or_default();
                elements.insert(0, head);
                let label = self.intern_static_list(elements);
                Some(StaticValue::StaticList { label })
            }
            _ => None,
        }
    }

    fn intern_static_record(
        &mut self,
        name: String,
        fields: Vec<(String, StaticValue)>,
    ) -> RecordLabel {
        let label = RecordLabel(self.static_records.len());
        self.static_records.push(StaticRecord { name, fields });
        label
    }

    fn intern_static_list(&mut self, elements: Vec<StaticValue>) -> ListLabel {
        let label = ListLabel(self.static_lists.len());
        self.static_lists.push(StaticList { elements });
        label
    }

    fn intern_static_map(&mut self, entries: Vec<(StaticValue, StaticValue)>) -> MapLabel {
        let label = MapLabel(self.static_maps.len());
        self.static_maps.push(StaticMap { entries });
        label
    }

    fn intern_static_set(&mut self, elements: Vec<StaticValue>) -> SetLabel {
        let label = SetLabel(self.static_sets.len());
        self.static_sets.push(StaticSet { elements });
        label
    }

    fn intern_static_lambda(
        &mut self,
        params: Vec<String>,
        body: Expr,
        captures: HashMap<String, StaticValue>,
        runtime_captures: HashMap<String, VarSlot>,
        contains_thread_call: bool,
    ) -> LambdaLabel {
        let label = LambdaLabel(self.static_lambdas.len());
        self.static_lambdas.push(StaticLambda {
            params,
            body,
            captures,
            runtime_captures,
            contains_thread_call,
        });
        label
    }

    fn static_lambda_value_for_function_name(&mut self, name: &str) -> Option<StaticValue> {
        let function = self.functions.get(name)?.clone();
        let label = self.intern_static_lambda(
            function.params,
            function.body,
            HashMap::new(),
            HashMap::new(),
            function.contains_thread_call,
        );
        Some(StaticValue::StaticLambda { label })
    }

    fn static_builtin_value_for_identifier(&mut self, name: &str) -> Option<StaticValue> {
        let canonical_name = self.canonical_builtin_name(name);
        if !Self::is_supported_builtin_alias_target(&canonical_name) {
            return None;
        }
        let label = self.intern_builtin_alias(canonical_name);
        Some(StaticValue::BuiltinFunction { label })
    }

    fn builtin_function_display_string(&self, label: BuiltinLabel) -> String {
        let name = self
            .builtin_aliases
            .get(label.0)
            .map(String::as_str)
            .unwrap_or("<unknown>");
        format!("<builtin:{name}>")
    }

    fn current_static_captures(&self) -> HashMap<String, StaticValue> {
        let mut captures = HashMap::new();
        for scope in &self.static_scopes {
            for (name, value) in scope {
                captures.insert(name.clone(), value.clone());
            }
        }
        captures
    }

    fn current_runtime_captures(&self) -> HashMap<String, VarSlot> {
        let mut captures = HashMap::new();
        for scope in &self.scopes {
            for (name, slot) in scope {
                captures.insert(name.clone(), *slot);
            }
        }
        captures
    }

    fn bind_static_lambda_captures(&mut self, lambda: &StaticLambda) {
        self.bind_static_lambda_runtime_captures(lambda);
        for (name, value) in lambda.captures.clone() {
            if self.static_capture_shadowed_by_runtime(lambda, &name) {
                continue;
            }
            self.bind_static_runtime_value(name, value);
        }
    }

    fn bind_static_lambda_runtime_captures(&mut self, lambda: &StaticLambda) {
        for (name, slot) in &lambda.runtime_captures {
            self.bind_existing_slot(name.clone(), *slot);
        }
    }

    fn bind_queued_thread_captures(&mut self, thread: &QueuedThread) {
        for (name, slot) in &thread.runtime_captures {
            self.bind_existing_slot(name.clone(), *slot);
        }
        for (name, value) in thread.captures.clone() {
            if thread
                .runtime_captures
                .get(&name)
                .is_some_and(|slot| slot.offset > 0)
            {
                continue;
            }
            self.bind_static_runtime_value(name, value);
        }
    }

    fn static_lambda_captures_current_scope(&self, label: LambdaLabel) -> bool {
        let Some(base_offset) = self.scope_base_offsets.last().copied() else {
            return false;
        };
        self.static_lambdas.get(label.0).is_some_and(|lambda| {
            lambda
                .runtime_captures
                .values()
                .any(|slot| slot.offset > base_offset)
        })
    }

    fn lambda_uses_runtime_captures(&self, lambda: &StaticLambda) -> bool {
        let mut names = lambda
            .runtime_captures
            .iter()
            .filter(|(_, slot)| slot.offset > 0)
            .map(|(name, _)| name.clone())
            .collect::<HashSet<_>>();
        for param in &lambda.params {
            names.remove(param);
        }
        expr_references_any_name(&lambda.body, &names)
    }

    fn static_capture_shadowed_by_runtime(&self, lambda: &StaticLambda, name: &str) -> bool {
        lambda
            .runtime_captures
            .get(name)
            .is_some_and(|slot| slot.offset > 0)
    }

    fn queued_threads_capture_current_scope(&self) -> bool {
        self.queued_threads
            .iter()
            .any(|thread| self.queued_thread_captures_current_scope(thread))
    }

    fn queued_thread_captures_current_scope(&self, thread: &QueuedThread) -> bool {
        let Some(base_offset) = self.scope_base_offsets.last().copied() else {
            return false;
        };
        let runtime_names = thread
            .runtime_captures
            .iter()
            .filter(|(_, slot)| slot.offset > base_offset)
            .map(|(name, _)| name.clone())
            .collect::<HashSet<_>>();
        if expr_references_any_name(&thread.body, &runtime_names) {
            return true;
        }
        thread.captures.iter().any(|(name, value)| {
            self.static_value_captures_current_scope(value) && {
                let mut names = HashSet::new();
                names.insert(name.clone());
                expr_references_any_name(&thread.body, &names)
            }
        })
    }

    fn native_value_captures_current_scope(&self, value: NativeValue) -> bool {
        match value {
            NativeValue::StaticLambda { label } => self.static_lambda_captures_current_scope(label),
            NativeValue::StaticList { label } => {
                self.static_lists.get(label.0).is_some_and(|list| {
                    list.elements
                        .iter()
                        .any(|value| self.static_value_captures_current_scope(value))
                })
            }
            NativeValue::StaticRecord { label } => {
                self.static_records.get(label.0).is_some_and(|record| {
                    record
                        .fields
                        .iter()
                        .any(|(_, value)| self.static_value_captures_current_scope(value))
                })
            }
            NativeValue::StaticMap { label } => self.static_maps.get(label.0).is_some_and(|map| {
                map.entries.iter().any(|(key, value)| {
                    self.static_value_captures_current_scope(key)
                        || self.static_value_captures_current_scope(value)
                })
            }),
            NativeValue::StaticSet { label } => self.static_sets.get(label.0).is_some_and(|set| {
                set.elements
                    .iter()
                    .any(|value| self.static_value_captures_current_scope(value))
            }),
            NativeValue::Int
            | NativeValue::Bool
            | NativeValue::Null
            | NativeValue::Unit
            | NativeValue::StaticFloat { .. }
            | NativeValue::StaticDouble { .. }
            | NativeValue::StaticString { .. }
            | NativeValue::RuntimeString { .. }
            | NativeValue::RuntimeLinesList { .. }
            | NativeValue::StaticIntList { .. }
            | NativeValue::BuiltinFunction { .. } => false,
        }
    }

    fn static_value_captures_current_scope(&self, value: &StaticValue) -> bool {
        match value {
            StaticValue::StaticLambda { label } => {
                self.static_lambda_captures_current_scope(*label)
            }
            StaticValue::StaticList { label } => {
                self.static_lists.get(label.0).is_some_and(|list| {
                    list.elements
                        .iter()
                        .any(|value| self.static_value_captures_current_scope(value))
                })
            }
            StaticValue::StaticRecord { label } => {
                self.static_records.get(label.0).is_some_and(|record| {
                    record
                        .fields
                        .iter()
                        .any(|(_, value)| self.static_value_captures_current_scope(value))
                })
            }
            StaticValue::StaticMap { label } => self.static_maps.get(label.0).is_some_and(|map| {
                map.entries.iter().any(|(key, value)| {
                    self.static_value_captures_current_scope(key)
                        || self.static_value_captures_current_scope(value)
                })
            }),
            StaticValue::StaticSet { label } => self.static_sets.get(label.0).is_some_and(|set| {
                set.elements
                    .iter()
                    .any(|value| self.static_value_captures_current_scope(value))
            }),
            StaticValue::Int(_)
            | StaticValue::Float(_)
            | StaticValue::Double(_)
            | StaticValue::Bool(_)
            | StaticValue::Null
            | StaticValue::Unit
            | StaticValue::StaticString { .. }
            | StaticValue::StaticIntList { .. }
            | StaticValue::BuiltinFunction { .. } => false,
        }
    }

    fn intern_builtin_alias(&mut self, name: String) -> BuiltinLabel {
        if let Some((index, _)) = self
            .builtin_aliases
            .iter()
            .enumerate()
            .find(|(_, existing)| existing.as_str() == name)
        {
            return BuiltinLabel(index);
        }
        let label = BuiltinLabel(self.builtin_aliases.len());
        self.builtin_aliases.push(name);
        label
    }

    fn builtin_alias_from_expr(&self, expr: &Expr) -> Option<String> {
        let Expr::Identifier { name, .. } = expr else {
            return None;
        };
        if self.functions.contains_key(name) {
            return None;
        }
        let canonical_name = self.canonical_builtin_name(name);
        if Self::is_supported_builtin_alias_target(&canonical_name) {
            return Some(canonical_name);
        }
        let NativeValue::BuiltinFunction { label } = self.lookup_var(name)?.value else {
            return None;
        };
        self.builtin_aliases.get(label.0).cloned()
    }

    fn current_thread_aliases(&self) -> HashSet<String> {
        let mut aliases = HashSet::from([String::from("thread")]);
        for scope in &self.scopes {
            for (name, slot) in scope {
                if let NativeValue::BuiltinFunction { label } = slot.value
                    && self
                        .builtin_aliases
                        .get(label.0)
                        .is_some_and(|alias| alias == "thread")
                {
                    aliases.insert(name.clone());
                }
            }
        }
        aliases
    }

    fn current_thread_aliases_with_static_bindings(
        &self,
        bindings: &[(&str, StaticValue)],
    ) -> HashSet<String> {
        let mut aliases = self.current_thread_aliases();
        for (name, value) in bindings {
            if let StaticValue::BuiltinFunction { label } = value
                && self
                    .builtin_aliases
                    .get(label.0)
                    .is_some_and(|alias| alias == "thread")
            {
                aliases.insert((*name).to_string());
            }
        }
        aliases
    }

    fn canonical_builtin_name(&self, name: &str) -> String {
        if name == "args" {
            return String::from("CommandLine#args");
        }
        if name == "exit" {
            return String::from("Process#exit");
        }
        if name == "stdin" {
            return String::from("StandardInput#all");
        }
        if name == "stdinLines" {
            return String::from("StandardInput#lines");
        }
        if name == "env" {
            return String::from("Environment#vars");
        }
        if name == "getEnv" {
            return String::from("Environment#get");
        }
        if name == "hasEnv" {
            return String::from("Environment#exists");
        }
        let Some((module, member)) = name.split_once('#') else {
            return name.to_string();
        };
        if let Some(path) = self.module_aliases.get(module)
            && Self::is_builtin_module(path)
        {
            return format!("{path}#{member}");
        }
        name.to_string()
    }

    fn builtin_name_for_identifier(&self, name: &str) -> String {
        if let Some(slot) = self.lookup_var(name)
            && let NativeValue::BuiltinFunction { label } = slot.value
        {
            return self.builtin_aliases[label.0].clone();
        }
        self.canonical_builtin_name(name)
    }

    fn is_builtin_module(path: &str) -> bool {
        matches!(
            path,
            "Map"
                | "Set"
                | "FileInput"
                | "FileOutput"
                | "StandardInput"
                | "Environment"
                | "CommandLine"
                | "Process"
                | "Dir"
        )
    }

    fn is_supported_builtin_alias_target(name: &str) -> bool {
        matches!(
            name,
            "println"
                | "printlnError"
                | "ToDo"
                | "sleep"
                | "thread"
                | "stopwatch"
                | "assert"
                | "assertResult"
                | "toString"
                | "substring"
                | "at"
                | "matches"
                | "split"
                | "trim"
                | "trimLeft"
                | "trimRight"
                | "replace"
                | "replaceAll"
                | "toLowerCase"
                | "toUpperCase"
                | "startsWith"
                | "endsWith"
                | "isEmptyString"
                | "indexOf"
                | "lastIndexOf"
                | "length"
                | "repeat"
                | "reverse"
                | "join"
                | "contains"
                | "double"
                | "sqrt"
                | "int"
                | "floor"
                | "ceil"
                | "abs"
                | "size"
                | "Map#size"
                | "Set#size"
                | "isEmpty"
                | "Map#isEmpty"
                | "Set#isEmpty"
                | "head"
                | "tail"
                | "cons"
                | "map"
                | "foldLeft"
                | "bind"
                | "unit"
                | "Map#containsKey"
                | "containsKey"
                | "Map#containsValue"
                | "containsValue"
                | "Map#get"
                | "get"
                | "Set#contains"
                | "FileOutput#write"
                | "FileOutput#append"
                | "FileOutput#writeLines"
                | "FileOutput#exists"
                | "FileOutput#delete"
                | "StandardInput#all"
                | "StandardInput#lines"
                | "Environment#vars"
                | "Environment#get"
                | "Environment#exists"
                | "CommandLine#args"
                | "Process#exit"
                | "FileInput#open"
                | "FileInput#all"
                | "FileInput#lines"
                | "FileInput#readAll"
                | "FileInput#readLines"
                | "Dir#current"
                | "Dir#home"
                | "Dir#temp"
                | "Dir#exists"
                | "Dir#mkdir"
                | "Dir#mkdirs"
                | "Dir#isDirectory"
                | "Dir#isFile"
                | "Dir#list"
                | "Dir#listFull"
                | "Dir#delete"
                | "Dir#copy"
                | "Dir#move"
        )
    }

    fn static_record_field(&self, label: RecordLabel, field: &str) -> Option<StaticValue> {
        self.static_records
            .get(label.0)?
            .fields
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value.clone())
    }

    fn static_values_equal(&self, expected: NativeValue, actual: NativeValue) -> Option<bool> {
        match (expected, actual) {
            (
                NativeValue::StaticString {
                    label: expected,
                    len,
                },
                NativeValue::StaticString {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => Some(
                self.asm.data_bytes_for_label(expected, len)
                    == self.asm.data_bytes_for_label(actual, actual_len),
            ),
            (
                NativeValue::StaticIntList {
                    label: expected,
                    len,
                },
                NativeValue::StaticIntList {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => {
                Some(self.asm.i64s_for_label(expected, len) == self.asm.i64s_for_label(actual, len))
            }
            (
                NativeValue::StaticIntList { label, len },
                NativeValue::StaticList { label: actual },
            ) => {
                let expected = self
                    .asm
                    .i64s_for_label(label, len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                Some(self.static_list_values_equal(&expected, actual))
            }
            (
                NativeValue::StaticList { label: expected },
                NativeValue::StaticIntList { label, len },
            ) => {
                let actual = self
                    .asm
                    .i64s_for_label(label, len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                Some(self.static_list_values_equal(&actual, expected))
            }
            (
                NativeValue::StaticList { label: expected },
                NativeValue::StaticList { label: actual },
            ) => Some(self.static_lists_equal(expected, actual)),
            (
                NativeValue::StaticRecord { label: expected },
                NativeValue::StaticRecord { label: actual },
            ) => Some(self.static_records_equal(expected, actual)),
            (
                NativeValue::StaticMap { label: expected },
                NativeValue::StaticMap { label: actual },
            ) => Some(self.static_maps_equal(expected, actual)),
            (
                NativeValue::StaticSet { label: expected },
                NativeValue::StaticSet { label: actual },
            ) => Some(self.static_sets_equal(expected, actual)),
            (
                NativeValue::StaticLambda { label: expected },
                NativeValue::StaticLambda { label: actual },
            ) => Some(self.static_lambdas_equal(expected, actual)),
            (
                NativeValue::BuiltinFunction { label: expected },
                NativeValue::BuiltinFunction { label: actual },
            ) => Some(self.builtin_aliases.get(expected.0) == self.builtin_aliases.get(actual.0)),
            (
                NativeValue::StaticFloat { bits: expected },
                NativeValue::StaticFloat { bits: actual },
            ) => Some(f32::from_bits(expected) == f32::from_bits(actual)),
            (
                NativeValue::StaticFloat { bits: expected },
                NativeValue::StaticDouble { bits: actual },
            ) => Some((f32::from_bits(expected) as f64) == f64::from_bits(actual)),
            (
                NativeValue::StaticDouble { bits: expected },
                NativeValue::StaticFloat { bits: actual },
            ) => Some(f64::from_bits(expected) == (f32::from_bits(actual) as f64)),
            (
                NativeValue::StaticDouble { bits: expected },
                NativeValue::StaticDouble { bits: actual },
            ) => Some(f64::from_bits(expected) == f64::from_bits(actual)),
            (NativeValue::Null, NativeValue::Null) => Some(true),
            (NativeValue::Unit, NativeValue::Unit) => Some(true),
            _ => None,
        }
    }

    fn static_values_equal_user(&self, expected: NativeValue, actual: NativeValue) -> Option<bool> {
        match (expected, actual) {
            (
                NativeValue::StaticString {
                    label: expected,
                    len,
                },
                NativeValue::StaticString {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => Some(
                self.asm.data_bytes_for_label(expected, len)
                    == self.asm.data_bytes_for_label(actual, actual_len),
            ),
            (
                NativeValue::StaticIntList {
                    label: expected,
                    len,
                },
                NativeValue::StaticIntList {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => {
                Some(self.asm.i64s_for_label(expected, len) == self.asm.i64s_for_label(actual, len))
            }
            (
                NativeValue::StaticIntList { label, len },
                NativeValue::StaticList { label: actual },
            ) => {
                let expected = self
                    .asm
                    .i64s_for_label(label, len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                Some(self.static_list_values_equal_user(&expected, actual))
            }
            (
                NativeValue::StaticList { label: expected },
                NativeValue::StaticIntList { label, len },
            ) => {
                let actual = self
                    .asm
                    .i64s_for_label(label, len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                Some(self.static_list_values_equal_user(&actual, expected))
            }
            (
                NativeValue::StaticList { label: expected },
                NativeValue::StaticList { label: actual },
            ) => Some(self.static_lists_equal_user(expected, actual)),
            (
                NativeValue::StaticRecord { label: expected },
                NativeValue::StaticRecord { label: actual },
            ) => Some(self.static_records_equal_user(expected, actual)),
            (
                NativeValue::StaticMap { label: expected },
                NativeValue::StaticMap { label: actual },
            ) => Some(self.static_maps_equal_user(expected, actual)),
            (
                NativeValue::StaticSet { label: expected },
                NativeValue::StaticSet { label: actual },
            ) => Some(self.static_sets_equal_user(expected, actual)),
            (
                NativeValue::StaticLambda { .. } | NativeValue::BuiltinFunction { .. },
                NativeValue::StaticLambda { .. } | NativeValue::BuiltinFunction { .. },
            ) => Some(false),
            (
                NativeValue::StaticFloat { bits: expected },
                NativeValue::StaticFloat { bits: actual },
            ) => Some(f32::from_bits(expected) == f32::from_bits(actual)),
            (
                NativeValue::StaticFloat { bits: expected },
                NativeValue::StaticDouble { bits: actual },
            ) => Some((f32::from_bits(expected) as f64) == f64::from_bits(actual)),
            (
                NativeValue::StaticDouble { bits: expected },
                NativeValue::StaticFloat { bits: actual },
            ) => Some(f64::from_bits(expected) == (f32::from_bits(actual) as f64)),
            (
                NativeValue::StaticDouble { bits: expected },
                NativeValue::StaticDouble { bits: actual },
            ) => Some(f64::from_bits(expected) == f64::from_bits(actual)),
            (NativeValue::Null, NativeValue::Null) => Some(true),
            (NativeValue::Unit, NativeValue::Unit) => Some(true),
            _ => None,
        }
    }

    fn static_records_equal(&self, expected: RecordLabel, actual: RecordLabel) -> bool {
        let Some(expected) = self.static_records.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_records.get(actual.0) else {
            return false;
        };
        expected.name == actual.name
            && expected.fields.len() == actual.fields.len()
            && expected.fields.iter().zip(actual.fields.iter()).all(
                |((expected_name, expected), (actual_name, actual))| {
                    expected_name == actual_name && self.static_value_equal(expected, actual)
                },
            )
    }

    fn static_records_equal_user(&self, expected: RecordLabel, actual: RecordLabel) -> bool {
        let Some(expected) = self.static_records.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_records.get(actual.0) else {
            return false;
        };
        expected.name == actual.name
            && expected.fields.len() == actual.fields.len()
            && expected.fields.iter().zip(actual.fields.iter()).all(
                |((expected_name, expected), (actual_name, actual))| {
                    expected_name == actual_name && self.static_value_equal_user(expected, actual)
                },
            )
    }

    fn static_lists_equal(&self, expected: ListLabel, actual: ListLabel) -> bool {
        let Some(expected) = self.static_lists.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_lists.get(actual.0) else {
            return false;
        };
        expected.elements.len() == actual.elements.len()
            && expected
                .elements
                .iter()
                .zip(actual.elements.iter())
                .all(|(expected, actual)| self.static_value_equal(expected, actual))
    }

    fn static_lists_equal_user(&self, expected: ListLabel, actual: ListLabel) -> bool {
        let Some(expected) = self.static_lists.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_lists.get(actual.0) else {
            return false;
        };
        expected.elements.len() == actual.elements.len()
            && expected
                .elements
                .iter()
                .zip(actual.elements.iter())
                .all(|(expected, actual)| self.static_value_equal_user(expected, actual))
    }

    fn static_maps_equal(&self, expected: MapLabel, actual: MapLabel) -> bool {
        let Some(expected) = self.static_maps.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_maps.get(actual.0) else {
            return false;
        };
        expected.entries.len() == actual.entries.len()
            && expected.entries.iter().zip(actual.entries.iter()).all(
                |((expected_key, expected_value), (actual_key, actual_value))| {
                    self.static_value_equal(expected_key, actual_key)
                        && self.static_value_equal(expected_value, actual_value)
                },
            )
    }

    fn static_maps_equal_user(&self, expected: MapLabel, actual: MapLabel) -> bool {
        let Some(expected) = self.static_maps.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_maps.get(actual.0) else {
            return false;
        };
        expected.entries.len() == actual.entries.len()
            && expected.entries.iter().zip(actual.entries.iter()).all(
                |((expected_key, expected_value), (actual_key, actual_value))| {
                    self.static_value_equal_user(expected_key, actual_key)
                        && self.static_value_equal_user(expected_value, actual_value)
                },
            )
    }

    fn static_sets_equal(&self, expected: SetLabel, actual: SetLabel) -> bool {
        let Some(expected) = self.static_sets.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_sets.get(actual.0) else {
            return false;
        };
        expected.elements.len() == actual.elements.len()
            && expected
                .elements
                .iter()
                .zip(actual.elements.iter())
                .all(|(expected, actual)| self.static_value_equal(expected, actual))
    }

    fn static_sets_equal_user(&self, expected: SetLabel, actual: SetLabel) -> bool {
        let Some(expected) = self.static_sets.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_sets.get(actual.0) else {
            return false;
        };
        expected.elements.len() == actual.elements.len()
            && expected
                .elements
                .iter()
                .zip(actual.elements.iter())
                .all(|(expected, actual)| self.static_value_equal_user(expected, actual))
    }

    fn static_value_equal(&self, expected: &StaticValue, actual: &StaticValue) -> bool {
        match (expected, actual) {
            (StaticValue::Int(expected), StaticValue::Int(actual)) => expected == actual,
            (StaticValue::Float(expected), StaticValue::Float(actual)) => {
                f32::from_bits(*expected) == f32::from_bits(*actual)
            }
            (StaticValue::Double(expected), StaticValue::Double(actual)) => {
                f64::from_bits(*expected) == f64::from_bits(*actual)
            }
            (StaticValue::Int(expected), StaticValue::Float(actual)) => {
                (*expected as f32) == f32::from_bits(*actual)
            }
            (StaticValue::Float(expected), StaticValue::Int(actual)) => {
                f32::from_bits(*expected) == (*actual as f32)
            }
            (StaticValue::Int(expected), StaticValue::Double(actual)) => {
                (*expected as f64) == f64::from_bits(*actual)
            }
            (StaticValue::Double(expected), StaticValue::Int(actual)) => {
                f64::from_bits(*expected) == (*actual as f64)
            }
            (StaticValue::Float(expected), StaticValue::Double(actual)) => {
                (f32::from_bits(*expected) as f64) == f64::from_bits(*actual)
            }
            (StaticValue::Double(expected), StaticValue::Float(actual)) => {
                f64::from_bits(*expected) == (f32::from_bits(*actual) as f64)
            }
            (StaticValue::Bool(expected), StaticValue::Bool(actual)) => expected == actual,
            (StaticValue::Null, StaticValue::Null) => true,
            (StaticValue::Unit, StaticValue::Unit) => true,
            (
                StaticValue::StaticString {
                    label: expected,
                    len,
                },
                StaticValue::StaticString {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => {
                self.asm.data_bytes_for_label(*expected, *len)
                    == self.asm.data_bytes_for_label(*actual, *actual_len)
            }
            (
                StaticValue::StaticIntList {
                    label: expected,
                    len,
                },
                StaticValue::StaticIntList {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => {
                self.asm.i64s_for_label(*expected, *len)
                    == self.asm.i64s_for_label(*actual, *actual_len)
            }
            (
                StaticValue::StaticIntList { label, len },
                StaticValue::StaticList { label: actual },
            ) => {
                let expected = self
                    .asm
                    .i64s_for_label(*label, *len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                self.static_list_values_equal(&expected, *actual)
            }
            (
                StaticValue::StaticList { label: expected },
                StaticValue::StaticIntList { label, len },
            ) => {
                let actual = self
                    .asm
                    .i64s_for_label(*label, *len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                self.static_list_values_equal(&actual, *expected)
            }
            (
                StaticValue::StaticList { label: expected },
                StaticValue::StaticList { label: actual },
            ) => self.static_lists_equal(*expected, *actual),
            (
                StaticValue::StaticRecord { label: expected },
                StaticValue::StaticRecord { label: actual },
            ) => self.static_records_equal(*expected, *actual),
            (
                StaticValue::StaticMap { label: expected },
                StaticValue::StaticMap { label: actual },
            ) => self.static_maps_equal(*expected, *actual),
            (
                StaticValue::StaticSet { label: expected },
                StaticValue::StaticSet { label: actual },
            ) => self.static_sets_equal(*expected, *actual),
            (
                StaticValue::StaticLambda { label: expected },
                StaticValue::StaticLambda { label: actual },
            ) => self.static_lambdas_equal(*expected, *actual),
            (
                StaticValue::BuiltinFunction { label: expected },
                StaticValue::BuiltinFunction { label: actual },
            ) => self.builtin_aliases.get(expected.0) == self.builtin_aliases.get(actual.0),
            _ => false,
        }
    }

    fn static_value_equal_user(&self, expected: &StaticValue, actual: &StaticValue) -> bool {
        match (expected, actual) {
            (StaticValue::Int(expected), StaticValue::Int(actual)) => expected == actual,
            (StaticValue::Float(expected), StaticValue::Float(actual)) => {
                f32::from_bits(*expected) == f32::from_bits(*actual)
            }
            (StaticValue::Double(expected), StaticValue::Double(actual)) => {
                f64::from_bits(*expected) == f64::from_bits(*actual)
            }
            (StaticValue::Int(expected), StaticValue::Float(actual)) => {
                (*expected as f32) == f32::from_bits(*actual)
            }
            (StaticValue::Float(expected), StaticValue::Int(actual)) => {
                f32::from_bits(*expected) == (*actual as f32)
            }
            (StaticValue::Int(expected), StaticValue::Double(actual)) => {
                (*expected as f64) == f64::from_bits(*actual)
            }
            (StaticValue::Double(expected), StaticValue::Int(actual)) => {
                f64::from_bits(*expected) == (*actual as f64)
            }
            (StaticValue::Float(expected), StaticValue::Double(actual)) => {
                (f32::from_bits(*expected) as f64) == f64::from_bits(*actual)
            }
            (StaticValue::Double(expected), StaticValue::Float(actual)) => {
                f64::from_bits(*expected) == (f32::from_bits(*actual) as f64)
            }
            (StaticValue::Bool(expected), StaticValue::Bool(actual)) => expected == actual,
            (StaticValue::Null, StaticValue::Null) => true,
            (StaticValue::Unit, StaticValue::Unit) => true,
            (
                StaticValue::StaticString {
                    label: expected,
                    len,
                },
                StaticValue::StaticString {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => {
                self.asm.data_bytes_for_label(*expected, *len)
                    == self.asm.data_bytes_for_label(*actual, *actual_len)
            }
            (
                StaticValue::StaticIntList {
                    label: expected,
                    len,
                },
                StaticValue::StaticIntList {
                    label: actual,
                    len: actual_len,
                },
            ) if len == actual_len => {
                self.asm.i64s_for_label(*expected, *len)
                    == self.asm.i64s_for_label(*actual, *actual_len)
            }
            (
                StaticValue::StaticIntList { label, len },
                StaticValue::StaticList { label: actual },
            ) => {
                let expected = self
                    .asm
                    .i64s_for_label(*label, *len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                self.static_list_values_equal_user(&expected, *actual)
            }
            (
                StaticValue::StaticList { label: expected },
                StaticValue::StaticIntList { label, len },
            ) => {
                let actual = self
                    .asm
                    .i64s_for_label(*label, *len)
                    .into_iter()
                    .map(StaticValue::Int)
                    .collect::<Vec<_>>();
                self.static_list_values_equal_user(&actual, *expected)
            }
            (
                StaticValue::StaticList { label: expected },
                StaticValue::StaticList { label: actual },
            ) => self.static_lists_equal_user(*expected, *actual),
            (
                StaticValue::StaticRecord { label: expected },
                StaticValue::StaticRecord { label: actual },
            ) => self.static_records_equal_user(*expected, *actual),
            (
                StaticValue::StaticMap { label: expected },
                StaticValue::StaticMap { label: actual },
            ) => self.static_maps_equal_user(*expected, *actual),
            (
                StaticValue::StaticSet { label: expected },
                StaticValue::StaticSet { label: actual },
            ) => self.static_sets_equal_user(*expected, *actual),
            (
                StaticValue::StaticLambda { .. } | StaticValue::BuiltinFunction { .. },
                StaticValue::StaticLambda { .. } | StaticValue::BuiltinFunction { .. },
            ) => false,
            _ => false,
        }
    }

    fn static_lambdas_equal(&self, expected: LambdaLabel, actual: LambdaLabel) -> bool {
        if expected == actual {
            return true;
        }
        let Some(expected) = self.static_lambdas.get(expected.0) else {
            return false;
        };
        let Some(actual) = self.static_lambdas.get(actual.0) else {
            return false;
        };
        expected.params == actual.params
            && expected.contains_thread_call == actual.contains_thread_call
            && expr_shape_equal(&expected.body, &actual.body)
            && self.static_lambda_runtime_captures_equal(expected, actual)
            && self.static_lambda_static_captures_equal(expected, actual)
    }

    fn static_lambda_runtime_captures_equal(
        &self,
        expected: &StaticLambda,
        actual: &StaticLambda,
    ) -> bool {
        expected
            .runtime_captures
            .keys()
            .chain(actual.runtime_captures.keys())
            .filter(|name| self.static_lambda_references_name(expected, name))
            .all(|name| expected.runtime_captures.get(name) == actual.runtime_captures.get(name))
    }

    fn static_lambda_static_captures_equal(
        &self,
        expected: &StaticLambda,
        actual: &StaticLambda,
    ) -> bool {
        expected
            .captures
            .keys()
            .chain(actual.captures.keys())
            .filter(|name| self.static_lambda_references_name(expected, name))
            .all(
                |name| match (expected.captures.get(name), actual.captures.get(name)) {
                    (Some(expected_value), Some(actual_value)) => {
                        self.static_value_equal(expected_value, actual_value)
                    }
                    (None, None) => true,
                    _ => false,
                },
            )
    }

    fn static_lambda_references_name(&self, lambda: &StaticLambda, name: &str) -> bool {
        if lambda.params.iter().any(|param| param == name) {
            return false;
        }
        let mut names = HashSet::new();
        names.insert(name.to_string());
        expr_references_any_name(&lambda.body, &names)
    }

    fn static_list_values_equal(&self, expected: &[StaticValue], actual: ListLabel) -> bool {
        let Some(actual) = self.static_lists.get(actual.0) else {
            return false;
        };
        expected.len() == actual.elements.len()
            && expected
                .iter()
                .zip(actual.elements.iter())
                .all(|(expected, actual)| self.static_value_equal(expected, actual))
    }

    fn static_list_values_equal_user(&self, expected: &[StaticValue], actual: ListLabel) -> bool {
        let Some(actual) = self.static_lists.get(actual.0) else {
            return false;
        };
        expected.len() == actual.elements.len()
            && expected
                .iter()
                .zip(actual.elements.iter())
                .all(|(expected, actual)| self.static_value_equal_user(expected, actual))
    }

    fn conditional_static_scopes(
        &self,
        before: &[HashMap<String, StaticValue>],
        after: &[HashMap<String, StaticValue>],
    ) -> Vec<HashMap<String, StaticValue>> {
        before
            .iter()
            .zip(after.iter())
            .map(|(before_scope, after_scope)| {
                before_scope
                    .iter()
                    .filter_map(|(name, before_value)| {
                        let after_value = after_scope.get(name)?;
                        self.static_value_equal(before_value, after_value)
                            .then(|| (name.clone(), before_value.clone()))
                    })
                    .collect()
            })
            .collect()
    }

    fn static_equality_from_exprs(&mut self, lhs: &Expr, rhs: &Expr) -> Option<bool> {
        if !static_expr_is_pure(lhs) || !static_expr_is_pure(rhs) {
            return None;
        }
        let lhs = self.static_value_from_expr(lhs)?;
        let rhs = self.static_value_from_expr(rhs)?;
        Some(self.static_value_equal_user(&lhs, &rhs))
    }

    fn static_equality_from_exprs_preserving_effects(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<Option<bool>, Diagnostic> {
        let before_static_scopes = self.static_scopes.clone();
        let lhs_preview = self.preview_static_value_after_effectful_eval(lhs);
        self.static_scopes = before_static_scopes.clone();
        let rhs_preview = self.preview_static_value_after_effectful_eval(rhs);
        self.static_scopes = before_static_scopes;
        if lhs_preview.is_none() || rhs_preview.is_none() {
            return Ok(None);
        }

        let lhs =
            self.static_value_from_argument_preserving_effects(lhs, span, "native equality lhs")?;
        let rhs =
            self.static_value_from_argument_preserving_effects(rhs, span, "native equality rhs")?;
        Ok(Some(self.static_value_equal_user(&lhs, &rhs)))
    }

    fn static_numeric_binary_from_exprs(
        &mut self,
        lhs: &Expr,
        op: BinaryOp,
        rhs: &Expr,
    ) -> Option<StaticValue> {
        if !static_expr_is_pure(lhs) || !static_expr_is_pure(rhs) {
            return None;
        }
        let lhs = self.static_value_from_expr(lhs)?;
        let rhs = self.static_value_from_expr(rhs)?;
        match op {
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                static_numeric_binary_value(op, &lhs, &rhs)
            }
            BinaryOp::Less => Some(StaticValue::Bool(
                static_value_as_f64(&lhs)? < static_value_as_f64(&rhs)?,
            )),
            BinaryOp::LessEqual => Some(StaticValue::Bool(
                static_value_as_f64(&lhs)? <= static_value_as_f64(&rhs)?,
            )),
            BinaryOp::Greater => Some(StaticValue::Bool(
                static_value_as_f64(&lhs)? > static_value_as_f64(&rhs)?,
            )),
            BinaryOp::GreaterEqual => Some(StaticValue::Bool(
                static_value_as_f64(&lhs)? >= static_value_as_f64(&rhs)?,
            )),
            _ => None,
        }
    }

    fn static_numeric_binary_from_exprs_preserving_effects(
        &mut self,
        lhs: &Expr,
        op: BinaryOp,
        rhs: &Expr,
        span: Span,
    ) -> Result<Option<StaticValue>, Diagnostic> {
        if !matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ) {
            return Ok(None);
        }

        let before_static_scopes = self.static_scopes.clone();
        let lhs_preview = self.preview_static_value_after_effectful_eval(lhs);
        self.static_scopes = before_static_scopes.clone();
        let rhs_preview = self.preview_static_value_after_effectful_eval(rhs);
        self.static_scopes = before_static_scopes;
        let (Some(lhs_preview), Some(rhs_preview)) = (lhs_preview, rhs_preview) else {
            return Ok(None);
        };
        if numeric_or_comparison_binary_value(op, &lhs_preview, &rhs_preview).is_none() {
            return Ok(None);
        }

        let lhs = self.static_value_from_argument_preserving_effects(
            lhs,
            span,
            "native numeric binary lhs",
        )?;
        let rhs = self.static_value_from_argument_preserving_effects(
            rhs,
            span,
            "native numeric binary rhs",
        )?;
        Ok(numeric_or_comparison_binary_value(op, &lhs, &rhs))
    }

    fn static_string_concat_text(&mut self, lhs: &Expr, rhs: &Expr) -> Option<String> {
        if !static_expr_is_pure(lhs) || !static_expr_is_pure(rhs) {
            return None;
        }
        let lhs = self.static_value_from_expr(lhs)?;
        let rhs = self.static_value_from_expr(rhs)?;
        if !matches!(lhs, StaticValue::StaticString { .. })
            && !matches!(rhs, StaticValue::StaticString { .. })
        {
            return None;
        }
        Some(format!(
            "{}{}",
            self.static_value_display_string(&lhs),
            self.static_value_display_string(&rhs)
        ))
    }

    fn compile_runtime_string_concat(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let lhs_string = self.compile_runtime_string_concat_fragment(lhs, span)?;
        let rhs_string = self.compile_runtime_string_concat_fragment(rhs, span)?;
        Ok(self.emit_runtime_string_concat(lhs_string, rhs_string, span))
    }

    fn compile_runtime_string_concat_fragment(
        &mut self,
        expr: &Expr,
        span: Span,
    ) -> Result<NativeStringRef, Diagnostic> {
        let value = self.compile_expr(expr)?;
        if let Some(value) = self.native_string_ref(value) {
            return Ok(value);
        }
        if let NativeValue::RuntimeLinesList { data, len } = value {
            let rendered = self.emit_runtime_lines_list_to_runtime_string(
                NativeStringRef {
                    data,
                    len: NativeStringLen::Runtime(len),
                },
                span,
            );
            return Ok(self
                .native_string_ref(rendered)
                .expect("rendered runtime line list should expose a string ref"));
        }
        if value == NativeValue::Int {
            return Ok(self.emit_i64_rax_to_runtime_string_ref(
                span,
                "string concatenation result exceeds 65536 bytes",
            ));
        }
        if value == NativeValue::Bool {
            return Ok(self.emit_bool_rax_to_runtime_string_ref(
                span,
                "string concatenation result exceeds 65536 bytes",
            ));
        }
        if let Some(static_value) = self.static_value_from_native(value) {
            let text = self.static_value_display_string(&static_value);
            let label = self.asm.data_label_with_bytes(text.as_bytes());
            return Ok(NativeStringRef {
                data: label,
                len: NativeStringLen::Immediate(text.len()),
            });
        }
        Err(unsupported(span, "native string concatenation"))
    }

    fn expr_may_yield_native_string(&mut self, expr: &Expr) -> bool {
        self.expr_may_yield_static_string(expr) || self.expr_may_yield_runtime_string(expr)
    }

    fn expr_may_yield_static_string(&mut self, expr: &Expr) -> bool {
        if matches!(
            self.static_value_from_expr_with_bindings_preserving_static_scopes(expr, &[]),
            Some(StaticValue::StaticString { .. })
        ) {
            return true;
        }
        match expr {
            Expr::String { .. } => true,
            Expr::Block { expressions, .. } => expressions
                .last()
                .is_some_and(|expr| self.expr_may_yield_static_string(expr)),
            Expr::Binary {
                lhs,
                op: BinaryOp::Add,
                rhs,
                ..
            } => self.expr_may_yield_static_string(lhs) || self.expr_may_yield_static_string(rhs),
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.expr_may_yield_static_string(then_branch)
                    || else_branch
                        .as_deref()
                        .is_some_and(|branch| self.expr_may_yield_static_string(branch))
            }
            _ => false,
        }
    }

    fn expr_may_yield_runtime_string(&self, expr: &Expr) -> bool {
        match expr {
            Expr::String { value, .. } if value.contains("#{") => true,
            Expr::Identifier { name, .. } => matches!(
                self.lookup_var(name).map(|slot| slot.value),
                Some(NativeValue::RuntimeString { .. })
            ),
            expr if self.file_input_all_print_call_name(expr).is_some() => true,
            Expr::Block { expressions, .. } => expressions
                .last()
                .is_some_and(|expr| self.expr_may_yield_runtime_string(expr)),
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.expr_may_yield_runtime_string(then_branch)
                    || else_branch
                        .as_deref()
                        .is_some_and(|branch| self.expr_may_yield_runtime_string(branch))
            }
            Expr::Binary {
                lhs,
                op: BinaryOp::Add,
                rhs,
                ..
            } => self.expr_may_yield_runtime_string(lhs) || self.expr_may_yield_runtime_string(rhs),
            Expr::Call {
                callee, arguments, ..
            } => match callee.as_ref() {
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "StandardInput#all" =>
                {
                    true
                }
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "Environment#get" =>
                {
                    true
                }
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "Dir#current" =>
                {
                    true
                }
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "Dir#home" =>
                {
                    true
                }
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "Dir#temp" =>
                {
                    true
                }
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "head" =>
                {
                    arguments
                        .first()
                        .is_some_and(|argument| self.expr_may_yield_runtime_lines_list(argument))
                }
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "join" =>
                {
                    arguments
                        .first()
                        .is_some_and(|argument| self.expr_may_yield_runtime_lines_list(argument))
                }
                Expr::Identifier { name, .. }
                    if self.builtin_name_for_identifier(name) == "toString" =>
                {
                    true
                }
                Expr::Identifier { name, .. } if runtime_string_returning_helper(name) => arguments
                    .first()
                    .is_some_and(|argument| self.expr_may_yield_runtime_string(argument)),
                Expr::FieldAccess { field, .. } if field == "toString" => true,
                Expr::FieldAccess { target, field, .. } if field == "head" => {
                    self.expr_may_yield_runtime_lines_list(target)
                }
                Expr::FieldAccess { target, field, .. } if field == "join" => {
                    self.expr_may_yield_runtime_lines_list(target)
                }
                Expr::FieldAccess { target, field, .. }
                    if runtime_string_returning_helper(field) =>
                {
                    self.expr_may_yield_runtime_string(target)
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn expr_may_yield_runtime_lines_list(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Identifier { name, .. } => matches!(
                self.lookup_var(name).map(|slot| slot.value),
                Some(NativeValue::RuntimeLinesList { .. })
            ),
            Expr::Block { expressions, .. } => expressions
                .last()
                .is_some_and(|expr| self.expr_may_yield_runtime_lines_list(expr)),
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.expr_may_yield_runtime_lines_list(then_branch)
                    || else_branch
                        .as_deref()
                        .is_some_and(|branch| self.expr_may_yield_runtime_lines_list(branch))
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                if let Expr::FieldAccess { target, field, .. } = callee.as_ref()
                    && field == "split"
                {
                    return self.expr_may_yield_runtime_string(target);
                }
                if let Some(name) = self.file_input_lines_print_call_name(expr) {
                    return matches!(name.as_str(), "FileInput#lines" | "FileInput#readLines")
                        && arguments
                            .first()
                            .is_some_and(|argument| self.expr_may_yield_runtime_string(argument));
                }
                if let Some((path, _)) = self.file_input_open_read_lines_print_call(expr) {
                    return self.expr_may_yield_runtime_string(path);
                }
                match callee.as_ref() {
                    Expr::Identifier { name, .. }
                        if self.builtin_name_for_identifier(name) == "StandardInput#lines" =>
                    {
                        true
                    }
                    Expr::Identifier { name, .. }
                        if self.builtin_name_for_identifier(name) == "Environment#vars" =>
                    {
                        true
                    }
                    Expr::Identifier { name, .. }
                        if self.builtin_name_for_identifier(name) == "CommandLine#args" =>
                    {
                        true
                    }
                    Expr::Identifier { name, .. }
                        if self.builtin_name_for_identifier(name) == "split" =>
                    {
                        arguments
                            .first()
                            .is_some_and(|argument| self.expr_may_yield_runtime_string(argument))
                    }
                    Expr::Identifier { name, .. }
                        if self.builtin_name_for_identifier(name) == "tail" =>
                    {
                        arguments.first().is_some_and(|argument| {
                            self.expr_may_yield_runtime_lines_list(argument)
                        })
                    }
                    Expr::Call {
                        callee: inner_callee,
                        arguments: head_arguments,
                        ..
                    } if arguments.len() == 1 && head_arguments.len() == 1 => {
                        if let Expr::Identifier { name, .. } = inner_callee.as_ref()
                            && self.builtin_name_for_identifier(name) == "cons"
                        {
                            return self.expr_may_yield_runtime_lines_list(&arguments[0]);
                        }
                        false
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn static_interpolated_string_value(&mut self, value: &str) -> Option<String> {
        self.static_interpolated_string_value_inner(value, &[], None)
    }

    fn static_interpolated_string_value_preserving_effects(
        &mut self,
        value: &str,
        span: Span,
    ) -> Option<String> {
        self.static_interpolated_string_value_inner(value, &[], Some(span))
    }

    fn static_interpolated_string_value_with_bindings(
        &mut self,
        value: &str,
        bindings: &[(&str, StaticValue)],
    ) -> Option<String> {
        self.static_interpolated_string_value_inner(value, bindings, None)
    }

    fn static_interpolated_string_value_inner(
        &mut self,
        value: &str,
        bindings: &[(&str, StaticValue)],
        preserve_effects_span: Option<Span>,
    ) -> Option<String> {
        let mut result = String::new();
        let chars = value.chars().collect::<Vec<_>>();
        let mut index = 0usize;
        while index < chars.len() {
            if chars[index] == '#' && chars.get(index + 1) == Some(&'{') {
                index += 2;
                let mut expr = String::new();
                let mut brace_depth = 0usize;
                let mut paren_depth = 0usize;
                let mut bracket_depth = 0usize;
                while index < chars.len() {
                    let ch = chars[index];
                    match ch {
                        '{' => {
                            brace_depth += 1;
                            expr.push(ch);
                        }
                        '}' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                            break;
                        }
                        '}' => {
                            brace_depth = brace_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        '(' => {
                            paren_depth += 1;
                            expr.push(ch);
                        }
                        ')' => {
                            paren_depth = paren_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        '[' => {
                            bracket_depth += 1;
                            expr.push(ch);
                        }
                        ']' => {
                            bracket_depth = bracket_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        _ => expr.push(ch),
                    }
                    index += 1;
                }
                if index >= chars.len() || chars[index] != '}' {
                    return None;
                }
                index += 1;
                let normalized = strip_dynamic_cast(expr.trim());
                let parsed = parse_inline_expression("<interpolation>", &normalized).ok()?;
                let parsed = rewrite_expression(parsed);
                let value = if let Some(span) = preserve_effects_span
                    && bindings.is_empty()
                {
                    self.static_value_from_argument_preserving_effects(
                        &parsed,
                        span,
                        "native string interpolation",
                    )
                    .ok()?
                } else if bindings.is_empty() {
                    self.static_value_from_expr(&parsed)?
                } else {
                    self.static_value_from_expr_with_bindings(&parsed, bindings)?
                };
                result.push_str(&self.static_value_display_string(&value));
            } else {
                result.push(chars[index]);
                index += 1;
            }
        }
        Some(result)
    }

    fn static_value_display_string(&self, value: &StaticValue) -> String {
        match value {
            StaticValue::Int(value) => value.to_string(),
            StaticValue::Float(bits) => format_static_float(*bits),
            StaticValue::Double(bits) => format_static_double(*bits),
            StaticValue::Bool(value) => value.to_string(),
            StaticValue::Null => "null".to_string(),
            StaticValue::Unit => "()".to_string(),
            StaticValue::StaticString { label, len } => {
                String::from_utf8_lossy(self.asm.data_bytes_for_label(*label, *len)).into_owned()
            }
            StaticValue::StaticIntList { label, len } => {
                let body = self
                    .asm
                    .i64s_for_label(*label, *len)
                    .into_iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{body}]")
            }
            StaticValue::StaticList { label } => self.static_list_display_string(*label),
            StaticValue::StaticRecord { label } => self.static_record_display_string(*label),
            StaticValue::StaticMap { label } => self.static_map_display_string(*label),
            StaticValue::StaticSet { label } => self.static_set_display_string(*label),
            StaticValue::StaticLambda { .. } => "<function>".to_string(),
            StaticValue::BuiltinFunction { label } => self.builtin_function_display_string(*label),
        }
    }

    fn static_record_display_string(&self, label: RecordLabel) -> String {
        let Some(record) = self.static_records.get(label.0) else {
            return "#()".to_string();
        };
        let body = record
            .fields
            .iter()
            .map(|(_, value)| self.static_value_display_string(value))
            .collect::<Vec<_>>()
            .join(", ");
        format!("#{}({body})", record.name)
    }

    fn static_list_display_string(&self, label: ListLabel) -> String {
        let Some(list) = self.static_lists.get(label.0) else {
            return "[]".to_string();
        };
        let body = list
            .elements
            .iter()
            .map(|value| self.static_value_display_string(value))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{body}]")
    }

    fn static_map_display_string(&self, label: MapLabel) -> String {
        let Some(map) = self.static_maps.get(label.0) else {
            return "%[]".to_string();
        };
        let body = map
            .entries
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}: {}",
                    self.static_value_display_string(key),
                    self.static_value_display_string(value)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("%[{body}]")
    }

    fn static_set_display_string(&self, label: SetLabel) -> String {
        let Some(set) = self.static_sets.get(label.0) else {
            return "%()".to_string();
        };
        let body = set
            .elements
            .iter()
            .map(|value| self.static_value_display_string(value))
            .collect::<Vec<_>>()
            .join(", ");
        format!("%({body})")
    }

    fn static_value_matches_annotation(
        &self,
        value: &StaticValue,
        annotation: &Option<String>,
    ) -> bool {
        let Some(annotation) = annotation.as_deref().map(str::trim) else {
            return true;
        };
        if annotation == "*" || annotation.starts_with('\'') {
            return true;
        }
        match value {
            StaticValue::Int(_) => matches!(annotation, "Byte" | "Short" | "Int" | "Long"),
            StaticValue::Float(_) => annotation == "Float",
            StaticValue::Double(_) => annotation == "Double",
            StaticValue::Bool(_) => matches!(annotation, "Boolean" | "Bool"),
            StaticValue::StaticString { .. } => annotation == "String",
            StaticValue::StaticIntList { .. } | StaticValue::StaticList { .. } => {
                annotation == "List" || annotation.starts_with("List<")
            }
            StaticValue::StaticRecord { label } => {
                let name = self
                    .static_records
                    .get(label.0)
                    .map(|record| record.name.as_str())
                    .unwrap_or("");
                annotation == name
                    || annotation == format!("#{name}")
                    || annotation.starts_with("record {")
                    || annotation == "*"
            }
            StaticValue::StaticMap { .. } => annotation == "Map" || annotation.starts_with("Map<"),
            StaticValue::StaticSet { .. } => annotation == "Set" || annotation.starts_with("Set<"),
            StaticValue::StaticLambda { .. } => annotation.contains("=>") || annotation == "*",
            StaticValue::BuiltinFunction { .. } => annotation.contains("=>") || annotation == "*",
            StaticValue::Null => true,
            StaticValue::Unit => annotation == "Unit",
        }
    }

    fn compile_if(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let else_label = self.asm.create_text_label();
        let end_label = self.asm.create_text_label();
        let branch_string_output = if let Some(else_branch) = else_branch
            && self.expr_may_yield_native_string(then_branch)
            && self.expr_may_yield_native_string(else_branch)
        {
            Some((
                self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]),
                self.asm.data_label_with_i64s(&[0]),
            ))
        } else {
            None
        };
        let branch_lines_output = if let Some(else_branch) = else_branch
            && self.expr_may_yield_runtime_lines_list(then_branch)
            && self.expr_may_yield_runtime_lines_list(else_branch)
        {
            Some((
                self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]),
                self.asm.data_label_with_i64s(&[0]),
            ))
        } else {
            None
        };
        let condition_value = self.compile_expr(condition)?;
        if condition_value != NativeValue::Bool {
            return Err(unsupported(span, "native if condition for non-Bool"));
        }
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, else_label);
        let before_scopes = self.scopes.clone();
        let before_static_scopes = self.static_scopes.clone();
        let before_scope_base_offsets = self.scope_base_offsets.clone();
        let before_next_stack_offset = self.next_stack_offset;
        let before_virtual_files = self.virtual_files.clone();
        let before_virtual_dirs = self.virtual_dirs.clone();
        let before_unknown_virtual_paths = self.unknown_virtual_paths.clone();
        let before_queued_threads = self.queued_threads.clone();
        self.dynamic_control_depth += 1;
        self.mergeable_dynamic_branch_depth += 1;
        self.push_scope();
        let then_value = self.compile_expr(then_branch)?;
        if self.native_value_captures_current_scope(then_value)
            || self.queued_threads_capture_current_scope()
        {
            self.pop_scope_preserving_allocations();
        } else {
            self.pop_scope();
        }
        if let Some((data, len)) = branch_string_output
            && let Some(input) = self.native_string_ref(then_value)
        {
            self.emit_copy_native_string_to_runtime_string_buffer(
                data,
                len,
                input,
                span,
                "if string result exceeds 65536 bytes",
            );
        }
        if let Some((data, len)) = branch_lines_output
            && let NativeValue::RuntimeLinesList {
                data: input_data,
                len: input_len,
            } = then_value
        {
            self.emit_copy_native_string_to_runtime_string_buffer(
                data,
                len,
                NativeStringRef {
                    data: input_data,
                    len: NativeStringLen::Runtime(input_len),
                },
                span,
                "if line-list result exceeds 65536 bytes",
            );
        }
        let then_next_stack_offset = self.next_stack_offset;
        let then_scopes = self.scopes.clone();
        let then_static_scopes = self.static_scopes.clone();
        let then_virtual_files = self.virtual_files.clone();
        let then_virtual_dirs = self.virtual_dirs.clone();
        let then_unknown_virtual_paths = self.unknown_virtual_paths.clone();
        let then_queued_threads = self.queued_threads.clone();
        self.scopes = before_scopes.clone();
        self.static_scopes = before_static_scopes.clone();
        self.scope_base_offsets = before_scope_base_offsets.clone();
        self.next_stack_offset = before_next_stack_offset;
        self.virtual_files = before_virtual_files;
        self.virtual_dirs = before_virtual_dirs;
        self.unknown_virtual_paths = before_unknown_virtual_paths;
        self.queued_threads = before_queued_threads;
        self.asm.jmp_label(end_label);
        self.asm.bind_text_label(else_label);
        self.push_scope();
        let else_value = if let Some(else_branch) = else_branch {
            self.compile_expr(else_branch)?
        } else {
            NativeValue::Unit
        };
        if self.native_value_captures_current_scope(else_value)
            || self.queued_threads_capture_current_scope()
        {
            self.pop_scope_preserving_allocations();
        } else {
            self.pop_scope();
        }
        if let Some((data, len)) = branch_string_output
            && let Some(input) = self.native_string_ref(else_value)
        {
            self.emit_copy_native_string_to_runtime_string_buffer(
                data,
                len,
                input,
                span,
                "if string result exceeds 65536 bytes",
            );
        }
        if let Some((data, len)) = branch_lines_output
            && let NativeValue::RuntimeLinesList {
                data: input_data,
                len: input_len,
            } = else_value
        {
            self.emit_copy_native_string_to_runtime_string_buffer(
                data,
                len,
                NativeStringRef {
                    data: input_data,
                    len: NativeStringLen::Runtime(input_len),
                },
                span,
                "if line-list result exceeds 65536 bytes",
            );
        }
        let else_next_stack_offset = self.next_stack_offset;
        let else_scopes = self.scopes.clone();
        let else_static_scopes = self.static_scopes.clone();
        let else_virtual_files = self.virtual_files.clone();
        let else_virtual_dirs = self.virtual_dirs.clone();
        let else_unknown_virtual_paths = self.unknown_virtual_paths.clone();
        let else_queued_threads = self.queued_threads.clone();
        self.mergeable_dynamic_branch_depth -= 1;
        self.dynamic_control_depth -= 1;
        self.scopes =
            self.merge_dynamic_branch_scopes(&before_scopes, &then_scopes, &else_scopes, span)?;
        self.static_scopes =
            self.merge_dynamic_branch_static_scopes(&then_static_scopes, &else_static_scopes);
        self.merge_dynamic_branch_virtual_state(
            &then_virtual_files,
            &else_virtual_files,
            &then_virtual_dirs,
            &else_virtual_dirs,
            &then_unknown_virtual_paths,
            &else_unknown_virtual_paths,
        );
        self.queued_threads = self.merge_dynamic_branch_queued_threads(
            &then_queued_threads,
            &else_queued_threads,
            span,
        )?;
        let then_preserved_stack = then_next_stack_offset - before_next_stack_offset;
        let else_preserved_stack = else_next_stack_offset - before_next_stack_offset;
        if then_preserved_stack != else_preserved_stack {
            return Err(unsupported(
                span,
                "native if branches with incompatible captured stack state",
            ));
        }
        self.scope_base_offsets = before_scope_base_offsets;
        self.next_stack_offset = before_next_stack_offset + then_preserved_stack;
        self.asm.bind_text_label(end_label);
        if then_value == else_value
            || self
                .static_values_equal(then_value, else_value)
                .unwrap_or(false)
        {
            Ok(then_value)
        } else if let Some((data, len)) = branch_string_output
            && self.native_string_ref(then_value).is_some()
            && self.native_string_ref(else_value).is_some()
        {
            Ok(NativeValue::RuntimeString { data, len })
        } else if let Some((data, len)) = branch_lines_output
            && matches!(then_value, NativeValue::RuntimeLinesList { .. })
            && matches!(else_value, NativeValue::RuntimeLinesList { .. })
        {
            Ok(NativeValue::RuntimeLinesList { data, len })
        } else if matches!(then_value, NativeValue::Unit) || matches!(else_value, NativeValue::Unit)
        {
            Ok(NativeValue::Unit)
        } else {
            Err(unsupported(
                span,
                "native if branches with different value types",
            ))
        }
    }

    fn merge_dynamic_branch_scopes(
        &self,
        before: &[HashMap<String, VarSlot>],
        then_scopes: &[HashMap<String, VarSlot>],
        else_scopes: &[HashMap<String, VarSlot>],
        span: Span,
    ) -> Result<Vec<HashMap<String, VarSlot>>, Diagnostic> {
        let mut merged = before.to_vec();
        for ((merged_scope, then_scope), else_scope) in merged
            .iter_mut()
            .zip(then_scopes.iter())
            .zip(else_scopes.iter())
        {
            for (name, merged_slot) in merged_scope.iter_mut() {
                let Some(then_slot) = then_scope.get(name) else {
                    continue;
                };
                let Some(else_slot) = else_scope.get(name) else {
                    continue;
                };
                if then_slot.offset != merged_slot.offset || else_slot.offset != merged_slot.offset
                {
                    return Err(unsupported(
                        span,
                        "native if branch state with incompatible variable storage",
                    ));
                }
                if self.native_values_mergeable(then_slot.value, else_slot.value) {
                    merged_slot.value = then_slot.value;
                } else {
                    return Err(unsupported(
                        span,
                        "native if branches with incompatible static variable values",
                    ));
                }
            }
        }
        Ok(merged)
    }

    fn merge_dynamic_branch_static_scopes(
        &self,
        then_scopes: &[HashMap<String, StaticValue>],
        else_scopes: &[HashMap<String, StaticValue>],
    ) -> Vec<HashMap<String, StaticValue>> {
        then_scopes
            .iter()
            .zip(else_scopes.iter())
            .map(|(then_scope, else_scope)| {
                then_scope
                    .iter()
                    .filter_map(|(name, then_value)| {
                        let else_value = else_scope.get(name)?;
                        self.static_value_equal(then_value, else_value)
                            .then(|| (name.clone(), then_value.clone()))
                    })
                    .collect()
            })
            .collect()
    }

    fn merge_dynamic_branch_virtual_state(
        &mut self,
        then_files: &HashMap<String, String>,
        else_files: &HashMap<String, String>,
        then_dirs: &HashSet<String>,
        else_dirs: &HashSet<String>,
        then_unknown: &HashSet<String>,
        else_unknown: &HashSet<String>,
    ) {
        let mut merged_files = HashMap::new();
        let mut unknown_paths = then_unknown
            .union(else_unknown)
            .cloned()
            .collect::<HashSet<_>>();
        for path in then_files.keys().chain(else_files.keys()) {
            match (then_files.get(path), else_files.get(path)) {
                (Some(then_content), Some(else_content)) if then_content == else_content => {
                    merged_files.insert(path.clone(), then_content.clone());
                    unknown_paths.remove(path);
                }
                (Some(_), _) | (_, Some(_)) => {
                    unknown_paths.insert(path.clone());
                }
                _ => {}
            }
        }

        let mut merged_dirs = HashSet::new();
        for path in then_dirs.intersection(else_dirs) {
            merged_dirs.insert(path.clone());
            unknown_paths.remove(path);
        }
        for path in then_dirs.symmetric_difference(else_dirs) {
            unknown_paths.insert(path.clone());
        }

        self.virtual_files = merged_files;
        self.virtual_dirs = merged_dirs;
        self.unknown_virtual_paths = unknown_paths;
    }

    fn merge_dynamic_branch_queued_threads(
        &self,
        then_threads: &[QueuedThread],
        else_threads: &[QueuedThread],
        span: Span,
    ) -> Result<Vec<QueuedThread>, Diagnostic> {
        if self.queued_threads_equal(then_threads, else_threads) {
            Ok(then_threads.to_vec())
        } else {
            Err(unsupported(
                span,
                "native thread queue inside divergent dynamic branches",
            ))
        }
    }

    fn queued_threads_equal(&self, lhs: &[QueuedThread], rhs: &[QueuedThread]) -> bool {
        lhs.len() == rhs.len()
            && lhs
                .iter()
                .zip(rhs.iter())
                .all(|(lhs, rhs)| self.queued_thread_equal(lhs, rhs))
    }

    fn queued_thread_equal(&self, lhs: &QueuedThread, rhs: &QueuedThread) -> bool {
        expr_shape_equal(&lhs.body, &rhs.body)
            && lhs.runtime_captures == rhs.runtime_captures
            && lhs.captures.len() == rhs.captures.len()
            && lhs.captures.iter().all(|(name, lhs_value)| {
                rhs.captures
                    .get(name)
                    .is_some_and(|rhs_value| self.static_value_equal(lhs_value, rhs_value))
            })
    }

    fn native_values_mergeable(&self, lhs: NativeValue, rhs: NativeValue) -> bool {
        lhs == rhs || self.static_values_equal(lhs, rhs).unwrap_or(false)
    }

    fn compile_statically_selected_if(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
        span: Span,
    ) -> Result<Option<NativeValue>, Diagnostic> {
        let before_static_scopes = self.static_scopes.clone();
        if let Some(StaticValue::Bool(condition_value)) =
            self.static_value_from_pure_expr(condition)
        {
            return if condition_value {
                Ok(Some(self.compile_expr(then_branch)?))
            } else if let Some(else_branch) = else_branch {
                Ok(Some(self.compile_expr(else_branch)?))
            } else {
                Ok(Some(NativeValue::Unit))
            };
        }

        let preview_value = self.preview_static_value_after_effectful_eval(condition);
        self.static_scopes = before_static_scopes;
        let Some(StaticValue::Bool(condition_value)) = preview_value else {
            return Ok(None);
        };
        let condition_native = self.compile_expr(condition)?;
        if condition_native != NativeValue::Bool {
            return Err(unsupported(span, "native if condition for non-Bool"));
        }
        if condition_value {
            Ok(Some(self.compile_expr(then_branch)?))
        } else if let Some(else_branch) = else_branch {
            Ok(Some(self.compile_expr(else_branch)?))
        } else {
            Ok(Some(NativeValue::Unit))
        }
    }

    fn compile_while(
        &mut self,
        condition: &Expr,
        body: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let before_condition_static_scopes = self.static_scopes.clone();
        let condition_preview = self.preview_static_value_after_effectful_eval(condition);
        self.static_scopes = before_condition_static_scopes;
        if matches!(condition_preview, Some(StaticValue::Bool(false))) {
            let condition_value = self.compile_expr(condition)?;
            if condition_value != NativeValue::Bool {
                return Err(unsupported(span, "native while condition for non-Bool"));
            }
            return Ok(NativeValue::Unit);
        }

        let mut assigned_names = assigned_names_in_expr(condition);
        assigned_names.extend(assigned_names_in_expr(body));
        let saved_static_scopes = self.static_scopes.clone();
        self.static_scopes = vec![HashMap::new(); saved_static_scopes.len()];
        let loop_label = self.asm.create_text_label();
        let end_label = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        let condition_value = self.compile_expr(condition)?;
        if condition_value != NativeValue::Bool {
            return Err(unsupported(span, "native while condition for non-Bool"));
        }
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, end_label);
        self.dynamic_control_depth += 1;
        self.push_scope();
        self.compile_expr(body)?;
        self.pop_scope();
        self.dynamic_control_depth -= 1;
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(end_label);
        self.static_scopes = saved_static_scopes;
        if !self.simulate_static_while(condition, body) {
            for name in assigned_names {
                self.remove_static_value(&name);
            }
        }
        Ok(NativeValue::Unit)
    }

    fn simulate_static_while(&mut self, condition: &Expr, body: &Expr) -> bool {
        for _ in 0..10_000 {
            match self.static_value_from_expr(condition) {
                Some(StaticValue::Bool(true)) => {
                    if self.static_value_from_expr(body).is_none() {
                        return false;
                    }
                }
                Some(StaticValue::Bool(false)) => return true,
                _ => return false,
            }
        }
        false
    }

    fn compile_foreach(
        &mut self,
        binding: &str,
        iterable: &Expr,
        body: &Expr,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let iterable_value = self.compile_expr(iterable)?;
        match iterable_value {
            NativeValue::StaticIntList { label, len } => {
                for element in self.asm.i64s_for_label(label, len) {
                    self.push_scope();
                    self.asm.mov_imm64(Reg::Rax, element as u64);
                    let slot = self.allocate_slot(binding.to_string(), NativeValue::Int);
                    self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                    self.bind_static_value(binding.to_string(), StaticValue::Int(element));
                    self.compile_expr(body)?;
                    if self.queued_threads_capture_current_scope() {
                        self.pop_scope_preserving_allocations();
                    } else {
                        self.pop_scope();
                    }
                }
            }
            NativeValue::StaticList { label } => {
                let elements = self
                    .static_lists
                    .get(label.0)
                    .map(|list| list.elements.clone())
                    .unwrap_or_default();
                for element in elements {
                    self.push_scope();
                    self.bind_static_iteration_value(binding, &element);
                    self.compile_expr(body)?;
                    if self.queued_threads_capture_current_scope() {
                        self.pop_scope_preserving_allocations();
                    } else {
                        self.pop_scope();
                    }
                }
            }
            NativeValue::RuntimeLinesList { data, len } => {
                let thread_aliases = self.current_thread_aliases();
                if expr_contains_thread_call(body, &thread_aliases) {
                    return Err(unsupported(
                        span,
                        "native foreach over runtime lines with thread body",
                    ));
                }
                let assigned_names = assigned_names_in_expr(body);
                let saved_static_scopes = self.static_scopes.clone();
                self.static_scopes = vec![HashMap::new(); saved_static_scopes.len()];

                const RUNTIME_STRING_CAP: usize = 65_536;
                let line_data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
                let line_len = self.asm.data_label_with_i64s(&[0]);
                let cursor = self.asm.data_label_with_i64s(&[0]);

                self.asm.mov_data_addr(Reg::Rax, cursor);
                self.asm.mov_imm64(Reg::R8, 0);
                self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);

                let loop_label = self.asm.create_text_label();
                let done = self.asm.create_text_label();
                let scan = self.asm.create_text_label();
                let segment_end = self.asm.create_text_label();
                let copy_loop = self.asm.create_text_label();
                let copied = self.asm.create_text_label();
                let consumed_at_end = self.asm.create_text_label();
                let body_label = self.asm.create_text_label();

                self.asm.bind_text_label(loop_label);
                self.asm.mov_data_addr(Reg::Rsi, data);
                self.emit_load_native_string_len(Reg::Rdx, NativeStringLen::Runtime(len));
                self.asm.mov_data_addr(Reg::Rax, cursor);
                self.asm.load_ptr_disp32(Reg::R9, Reg::Rax, 0);
                self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
                self.asm.jcc_label(Condition::GreaterEqual, done);
                self.asm.mov_reg_reg(Reg::R8, Reg::R9);

                self.asm.bind_text_label(scan);
                self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
                self.asm.jcc_label(Condition::Equal, segment_end);
                self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
                self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
                self.asm.jcc_label(Condition::Equal, segment_end);
                self.asm.inc_reg(Reg::R8);
                self.asm.jmp_label(scan);

                self.asm.bind_text_label(segment_end);
                self.asm.mov_imm64(Reg::R10, 0);
                self.asm.bind_text_label(copy_loop);
                self.asm.cmp_reg_reg(Reg::R9, Reg::R8);
                self.asm.jcc_label(Condition::Equal, copied);
                self.emit_runtime_buffer_capacity_check(
                    Reg::R10,
                    RUNTIME_STRING_CAP,
                    span,
                    "foreach runtime line exceeds 65536 bytes",
                );
                self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R9);
                self.asm.mov_data_addr(Reg::Rbx, line_data);
                self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
                self.asm.inc_reg(Reg::R9);
                self.asm.inc_reg(Reg::R10);
                self.asm.jmp_label(copy_loop);

                self.asm.bind_text_label(copied);
                self.asm.mov_data_addr(Reg::Rax, line_len);
                self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R10);
                self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
                self.asm.jcc_label(Condition::Equal, consumed_at_end);
                self.asm.inc_reg(Reg::R8);
                self.asm.jmp_label(body_label);

                self.asm.bind_text_label(consumed_at_end);
                self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);

                self.asm.bind_text_label(body_label);
                self.asm.mov_data_addr(Reg::Rax, cursor);
                self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);
                self.asm.mov_data_addr(Reg::Rsi, data);

                self.dynamic_control_depth += 1;
                self.push_scope();
                self.bind_constant(
                    binding.to_string(),
                    NativeValue::RuntimeString {
                        data: line_data,
                        len: line_len,
                    },
                );
                self.compile_expr(body)?;
                self.pop_scope();
                self.dynamic_control_depth -= 1;
                self.asm.jmp_label(loop_label);
                self.asm.bind_text_label(done);
                self.static_scopes = saved_static_scopes;
                for name in assigned_names {
                    self.remove_static_value(&name);
                }
            }
            _ => return Err(unsupported(span, "native foreach over non-static list")),
        }
        Ok(NativeValue::Unit)
    }

    fn bind_static_iteration_value(&mut self, binding: &str, value: &StaticValue) {
        let value = self.emit_static_value(value);
        match value {
            NativeValue::Int | NativeValue::Bool => {
                let slot = self.allocate_slot(binding.to_string(), value);
                self.asm.store_rbp_slot(slot.offset, Reg::Rax);
            }
            NativeValue::Unit
            | NativeValue::Null
            | NativeValue::StaticFloat { .. }
            | NativeValue::StaticDouble { .. }
            | NativeValue::StaticString { .. }
            | NativeValue::RuntimeString { .. }
            | NativeValue::RuntimeLinesList { .. }
            | NativeValue::StaticIntList { .. }
            | NativeValue::StaticList { .. }
            | NativeValue::StaticRecord { .. }
            | NativeValue::StaticMap { .. }
            | NativeValue::StaticSet { .. }
            | NativeValue::StaticLambda { .. }
            | NativeValue::BuiltinFunction { .. } => {
                self.bind_constant(binding.to_string(), value);
            }
        }
    }

    fn emit_print_expr_line(&mut self, fd: u64, expr: &Expr, span: Span) -> Result<(), Diagnostic> {
        self.emit_print_expr_fragment(fd, expr, span)?;
        self.emit_write_data(fd, self.newline, 1);
        Ok(())
    }

    fn emit_print_expr_fragment(
        &mut self,
        fd: u64,
        expr: &Expr,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if let Expr::String {
            value,
            span: string_span,
        } = expr
            && value.contains("#{")
        {
            return self.emit_print_interpolated_string(fd, value, *string_span);
        }

        if let Expr::Binary {
            lhs,
            op: BinaryOp::Add,
            rhs,
            ..
        } = expr
            && self.is_print_concat_expr(expr)
        {
            self.emit_print_expr_fragment(fd, lhs, span)?;
            self.emit_print_expr_fragment(fd, rhs, span)?;
            return Ok(());
        }

        if let Some(name) = self.file_input_all_print_call_name(expr) {
            let Expr::Call { arguments, .. } = expr else {
                unreachable!("file input print call matched only call expressions");
            };
            self.expect_static_arity(&name, arguments, 1, span)?;
            if self.expr_may_yield_runtime_string(&arguments[0]) {
                let path_label = self.compile_runtime_path_argument(&arguments[0], span, &name)?;
                self.emit_file_stream_to_fd_path_label(path_label, fd, span, &name);
                return Ok(());
            }
            let path = self.static_string_from_argument_preserving_effects(
                &arguments[0],
                span,
                &format!("native {name} for non-static path"),
            )?;
            self.emit_file_stream_to_fd(&path, fd, span, &name);
            return Ok(());
        }

        if let Some(name) = self.file_input_lines_print_call_name(expr) {
            let Expr::Call { arguments, .. } = expr else {
                unreachable!("file input lines print call matched only call expressions");
            };
            self.expect_static_arity(&name, arguments, 1, span)?;
            if self.expr_may_yield_runtime_string(&arguments[0]) {
                let path_label = self.compile_runtime_path_argument(&arguments[0], span, &name)?;
                let value =
                    self.emit_file_read_to_runtime_string_from_path_label(path_label, span, &name);
                let Some(input) = self.native_string_ref(value) else {
                    return Err(unsupported(span, "native FileInput#lines print"));
                };
                self.emit_print_runtime_lines_list(fd, input);
                return Ok(());
            }
        }

        if let Some((path, name)) = self.file_input_open_read_lines_print_call(expr)
            && self.expr_may_yield_runtime_string(path)
        {
            let path_label = self.compile_runtime_path_argument(path, span, "FileInput#open")?;
            let value =
                self.emit_file_read_to_runtime_string_from_path_label(path_label, span, &name);
            let Some(input) = self.native_string_ref(value) else {
                return Err(unsupported(span, "native FileInput#open readLines print"));
            };
            self.emit_print_runtime_lines_list(fd, input);
            return Ok(());
        }

        let value = self.compile_expr(expr)?;
        self.emit_print_value_fragment(fd, value);
        Ok(())
    }

    fn file_input_all_print_call_name(&self, expr: &Expr) -> Option<String> {
        let Expr::Call { callee, .. } = expr else {
            return None;
        };
        let Expr::Identifier { name, .. } = callee.as_ref() else {
            return None;
        };
        let name = self.builtin_name_for_identifier(name);
        matches!(name.as_str(), "FileInput#all" | "FileInput#readAll").then_some(name)
    }

    fn file_input_lines_print_call_name(&self, expr: &Expr) -> Option<String> {
        let Expr::Call { callee, .. } = expr else {
            return None;
        };
        let Expr::Identifier { name, .. } = callee.as_ref() else {
            return None;
        };
        let name = self.builtin_name_for_identifier(name);
        matches!(name.as_str(), "FileInput#lines" | "FileInput#readLines").then_some(name)
    }

    fn file_input_open_read_lines_print_call<'a>(
        &self,
        expr: &'a Expr,
    ) -> Option<(&'a Expr, String)> {
        let Expr::Call {
            callee, arguments, ..
        } = expr
        else {
            return None;
        };
        let Expr::Identifier { name, .. } = callee.as_ref() else {
            return None;
        };
        let name = self.builtin_name_for_identifier(name);
        if name != "FileInput#open" || arguments.len() != 2 {
            return None;
        }
        let callback = match &arguments[1] {
            Expr::Lambda { params, body, .. } => Some((params.as_slice(), body.as_ref())),
            Expr::Block { expressions, .. } if expressions.len() == 1 => {
                if let Expr::Lambda { params, body, .. } = &expressions[0] {
                    Some((params.as_slice(), body.as_ref()))
                } else {
                    None
                }
            }
            _ => None,
        }?;
        let ([param], body) = callback else {
            return None;
        };
        let name = self.file_input_open_callback_name(body, param)?;
        matches!(name.as_str(), "FileInput#lines" | "FileInput#readLines")
            .then_some((&arguments[0], name))
    }

    fn emit_print_interpolated_string(
        &mut self,
        fd: u64,
        value: &str,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let chars = value.chars().collect::<Vec<_>>();
        let mut literal = String::new();
        let mut index = 0usize;
        while index < chars.len() {
            if chars[index] == '#' && chars.get(index + 1) == Some(&'{') {
                if !literal.is_empty() {
                    let label = self.asm.data_label_with_bytes(literal.as_bytes());
                    self.emit_write_data(fd, label, literal.len());
                    literal.clear();
                }
                index += 2;
                let mut expr = String::new();
                let mut brace_depth = 0usize;
                let mut paren_depth = 0usize;
                let mut bracket_depth = 0usize;
                while index < chars.len() {
                    let ch = chars[index];
                    match ch {
                        '{' => {
                            brace_depth += 1;
                            expr.push(ch);
                        }
                        '}' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                            break;
                        }
                        '}' => {
                            brace_depth = brace_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        '(' => {
                            paren_depth += 1;
                            expr.push(ch);
                        }
                        ')' => {
                            paren_depth = paren_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        '[' => {
                            bracket_depth += 1;
                            expr.push(ch);
                        }
                        ']' => {
                            bracket_depth = bracket_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        _ => expr.push(ch),
                    }
                    index += 1;
                }
                if index >= chars.len() || chars[index] != '}' {
                    return Err(Diagnostic::compile(span, "unterminated interpolation"));
                }
                index += 1;
                let normalized = strip_dynamic_cast(expr.trim());
                let parsed =
                    parse_inline_expression("<interpolation>", &normalized).map_err(|_| {
                        Diagnostic::compile(span, "failed to parse interpolation expression")
                    })?;
                let parsed = rewrite_expression(parsed);
                self.emit_print_expr_fragment(fd, &parsed, span)?;
            } else {
                literal.push(chars[index]);
                index += 1;
            }
        }
        if !literal.is_empty() {
            let label = self.asm.data_label_with_bytes(literal.as_bytes());
            self.emit_write_data(fd, label, literal.len());
        }
        Ok(())
    }

    fn emit_runtime_interpolated_string(
        &mut self,
        value: &str,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);
        let chars = value.chars().collect::<Vec<_>>();
        let mut literal = String::new();
        let mut index = 0usize;
        while index < chars.len() {
            if chars[index] == '#' && chars.get(index + 1) == Some(&'{') {
                if !literal.is_empty() {
                    let label = self.asm.data_label_with_bytes(literal.as_bytes());
                    let input = NativeStringRef {
                        data: label,
                        len: NativeStringLen::Immediate(literal.len()),
                    };
                    self.emit_append_native_string_to_runtime_buffer_offset_label(
                        data,
                        offset,
                        input,
                        span,
                        "string interpolation result exceeds 65536 bytes",
                    );
                    literal.clear();
                }
                index += 2;
                let mut expr = String::new();
                let mut brace_depth = 0usize;
                let mut paren_depth = 0usize;
                let mut bracket_depth = 0usize;
                while index < chars.len() {
                    let ch = chars[index];
                    match ch {
                        '{' => {
                            brace_depth += 1;
                            expr.push(ch);
                        }
                        '}' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                            break;
                        }
                        '}' => {
                            brace_depth = brace_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        '(' => {
                            paren_depth += 1;
                            expr.push(ch);
                        }
                        ')' => {
                            paren_depth = paren_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        '[' => {
                            bracket_depth += 1;
                            expr.push(ch);
                        }
                        ']' => {
                            bracket_depth = bracket_depth.saturating_sub(1);
                            expr.push(ch);
                        }
                        _ => expr.push(ch),
                    }
                    index += 1;
                }
                if index >= chars.len() || chars[index] != '}' {
                    return Err(Diagnostic::compile(span, "unterminated interpolation"));
                }
                index += 1;
                let normalized = strip_dynamic_cast(expr.trim());
                let parsed =
                    parse_inline_expression("<interpolation>", &normalized).map_err(|_| {
                        Diagnostic::compile(span, "failed to parse interpolation expression")
                    })?;
                let parsed = rewrite_expression(parsed);
                let fragment = self.compile_expr(&parsed)?;
                if let Some(fragment) = self.native_string_ref(fragment) {
                    self.emit_append_native_string_to_runtime_buffer_offset_label(
                        data,
                        offset,
                        fragment,
                        span,
                        "string interpolation result exceeds 65536 bytes",
                    );
                } else if fragment == NativeValue::Int {
                    self.emit_append_i64_rax_to_runtime_buffer_offset_label(
                        data,
                        offset,
                        span,
                        "string interpolation result exceeds 65536 bytes",
                    );
                } else if fragment == NativeValue::Bool {
                    self.emit_append_bool_rax_to_runtime_buffer_offset_label(
                        data,
                        offset,
                        span,
                        "string interpolation result exceeds 65536 bytes",
                    );
                } else if let Some(static_value) = self.static_value_from_native(fragment) {
                    let text = self.static_value_display_string(&static_value);
                    let label = self.asm.data_label_with_bytes(text.as_bytes());
                    let fragment = NativeStringRef {
                        data: label,
                        len: NativeStringLen::Immediate(text.len()),
                    };
                    self.emit_append_native_string_to_runtime_buffer_offset_label(
                        data,
                        offset,
                        fragment,
                        span,
                        "string interpolation result exceeds 65536 bytes",
                    );
                } else {
                    return Err(unsupported(span, "native string interpolation"));
                }
            } else {
                literal.push(chars[index]);
                index += 1;
            }
        }
        if !literal.is_empty() {
            let label = self.asm.data_label_with_bytes(literal.as_bytes());
            let input = NativeStringRef {
                data: label,
                len: NativeStringLen::Immediate(literal.len()),
            };
            self.emit_append_native_string_to_runtime_buffer_offset_label(
                data,
                offset,
                input,
                span,
                "string interpolation result exceeds 65536 bytes",
            );
        }
        self.emit_store_runtime_string_len_from_offset(len, offset);
        Ok(NativeValue::RuntimeString { data, len })
    }

    fn emit_print_value_fragment(&mut self, fd: u64, value: NativeValue) {
        match value {
            NativeValue::Int => {
                self.asm.mov_reg_reg(Reg::Rdi, Reg::Rax);
                self.asm.mov_imm64(Reg::Rsi, fd);
                self.asm.mov_imm64(Reg::Rdx, 0);
                self.asm.call_label(self.print_i64);
            }
            NativeValue::Bool => self.emit_print_bool_fragment(fd),
            NativeValue::StaticFloat { bits } => {
                let text = format_static_float(bits);
                let label = self.asm.data_label_with_bytes(text.as_bytes());
                self.emit_write_data(fd, label, text.len());
            }
            NativeValue::StaticDouble { bits } => {
                let text = format_static_double(bits);
                let label = self.asm.data_label_with_bytes(text.as_bytes());
                self.emit_write_data(fd, label, text.len());
            }
            NativeValue::Null => {
                self.emit_write_data(fd, self.null_text, 4);
            }
            NativeValue::StaticString { label, len } => {
                self.emit_write_data(fd, label, len);
            }
            NativeValue::RuntimeString { data, len } => {
                self.emit_write_data_dynamic_len(fd, data, len);
            }
            NativeValue::RuntimeLinesList { data, len } => {
                self.emit_print_runtime_lines_list(
                    fd,
                    NativeStringRef {
                        data,
                        len: NativeStringLen::Runtime(len),
                    },
                );
            }
            NativeValue::StaticIntList { label, len } => {
                self.emit_print_static_int_list(fd, label, len);
            }
            NativeValue::StaticList { label } => {
                self.emit_print_static_list(fd, label);
            }
            NativeValue::StaticRecord { label } => {
                self.emit_print_static_record(fd, label);
            }
            NativeValue::StaticMap { label } => {
                self.emit_print_static_map(fd, label);
            }
            NativeValue::StaticSet { label } => {
                self.emit_print_static_set(fd, label);
            }
            NativeValue::StaticLambda { .. } => {
                let label = self.asm.data_label_with_bytes(b"<function>");
                self.emit_write_data(fd, label, "<function>".len());
            }
            NativeValue::Unit => {
                self.emit_write_data(fd, self.unit_text, 2);
            }
            NativeValue::BuiltinFunction { label } => {
                let text = self.builtin_function_display_string(label);
                let label = self.asm.data_label_with_bytes(text.as_bytes());
                self.emit_write_data(fd, label, text.len());
            }
        }
    }

    fn emit_print_static_int_list(&mut self, fd: u64, label: DataLabel, len: usize) {
        self.emit_write_data(fd, self.list_open, 1);
        for index in 0..len {
            if index > 0 {
                self.emit_write_data(fd, self.comma_space, 2);
            }
            self.asm.mov_data_addr(Reg::Rax, label);
            self.asm
                .load_ptr_disp32(Reg::Rax, Reg::Rax, (index * 8) as i32);
            self.asm.mov_reg_reg(Reg::Rdi, Reg::Rax);
            self.asm.mov_imm64(Reg::Rsi, fd);
            self.asm.mov_imm64(Reg::Rdx, 0);
            self.asm.call_label(self.print_i64);
        }
        self.emit_write_data(fd, self.list_close, 1);
    }

    fn emit_print_static_list(&mut self, fd: u64, label: ListLabel) {
        let Some(list) = self.static_lists.get(label.0).cloned() else {
            return;
        };
        self.emit_write_data(fd, self.list_open, 1);
        for (index, value) in list.elements.iter().enumerate() {
            if index > 0 {
                self.emit_write_data(fd, self.comma_space, 2);
            }
            let value = self.emit_static_value(value);
            self.emit_print_value_fragment(fd, value);
        }
        self.emit_write_data(fd, self.list_close, 1);
    }

    fn emit_print_runtime_lines_list(&mut self, fd: u64, input: NativeStringRef) {
        self.emit_write_data(fd, self.list_open, 1);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.mov_imm64(Reg::R10, 0);

        let scan = self.asm.create_text_label();
        let emit_line = self.asm.create_text_label();
        let finish = self.asm.create_text_label();
        let emit_final = self.asm.create_text_label();
        let close = self.asm.create_text_label();

        self.asm.bind_text_label(scan);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, finish);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, emit_line);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(emit_line);
        self.emit_print_runtime_line_segment(fd, input);
        self.asm.inc_reg(Reg::R8);
        self.asm.mov_reg_reg(Reg::R9, Reg::R8);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(finish);
        self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.jcc_label(Condition::Less, emit_final);
        self.asm.jmp_label(close);

        self.asm.bind_text_label(emit_final);
        self.emit_print_runtime_line_segment(fd, input);

        self.asm.bind_text_label(close);
        self.emit_write_data(fd, self.list_close, 1);
    }

    fn emit_print_runtime_line_segment(&mut self, fd: u64, input: NativeStringRef) {
        let no_separator = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::R10, 0);
        self.asm.jcc_label(Condition::Equal, no_separator);
        self.emit_write_data(fd, self.comma_space, 2);
        self.asm.bind_text_label(no_separator);
        self.asm.mov_imm64(Reg::R10, 1);

        self.asm.mov_reg_reg(Reg::Rdx, Reg::R8);
        self.asm.sub_reg_reg(Reg::Rdx, Reg::R9);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.mov_imm64(Reg::Rdi, fd);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.asm.add_reg_reg(Reg::Rsi, Reg::R9);
        self.asm.syscall();
    }

    fn emit_runtime_lines_list_to_runtime_string(
        &mut self,
        input: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);
        let cursor = self.asm.data_label_with_i64s(&[0]);
        let start = self.asm.data_label_with_i64s(&[0]);
        let end = self.asm.data_label_with_i64s(&[0]);
        let emitted = self.asm.data_label_with_i64s(&[0]);

        for label in [offset, cursor, start, end, emitted] {
            self.asm.mov_data_addr(Reg::R10, label);
            self.asm.mov_imm64(Reg::R8, 0);
            self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        }

        self.emit_append_native_string_to_runtime_buffer_offset_label(
            data,
            offset,
            NativeStringRef {
                data: self.list_open,
                len: NativeStringLen::Immediate(1),
            },
            span,
            "runtime line-list toString result exceeds 65536 bytes",
        );

        let scan = self.asm.create_text_label();
        let emit_line = self.asm.create_text_label();
        let finish = self.asm.create_text_label();
        let emit_final = self.asm.create_text_label();
        let close = self.asm.create_text_label();

        self.asm.bind_text_label(scan);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, finish);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, emit_line);
        self.asm.inc_reg(Reg::R8);
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(emit_line);
        self.asm.mov_data_addr(Reg::R10, end);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.emit_append_runtime_line_segment_to_runtime_string_buffer(
            data,
            offset,
            input,
            start,
            end,
            emitted,
            span,
            "runtime line-list toString result exceeds 65536 bytes",
        );
        self.asm.mov_data_addr(Reg::R10, cursor);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.inc_reg(Reg::R8);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::R10, start);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(finish);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::R10, start);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.jcc_label(Condition::Less, emit_final);
        self.asm.jmp_label(close);

        self.asm.bind_text_label(emit_final);
        self.asm.mov_data_addr(Reg::R10, end);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rdx);
        self.emit_append_runtime_line_segment_to_runtime_string_buffer(
            data,
            offset,
            input,
            start,
            end,
            emitted,
            span,
            "runtime line-list toString result exceeds 65536 bytes",
        );

        self.asm.bind_text_label(close);
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            data,
            offset,
            NativeStringRef {
                data: self.list_close,
                len: NativeStringLen::Immediate(1),
            },
            span,
            "runtime line-list toString result exceeds 65536 bytes",
        );
        self.emit_store_runtime_string_len_from_offset(len, offset);
        NativeValue::RuntimeString { data, len }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_append_runtime_line_segment_to_runtime_string_buffer(
        &mut self,
        output: DataLabel,
        offset: DataLabel,
        input: NativeStringRef,
        start: DataLabel,
        end: DataLabel,
        emitted: DataLabel,
        span: Span,
        overflow_message: &str,
    ) {
        let no_separator = self.asm.create_text_label();
        let copy_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.mov_data_addr(Reg::R10, emitted);
        self.asm.load_ptr_disp32(Reg::Rax, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, no_separator);
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            output,
            offset,
            NativeStringRef {
                data: self.comma_space,
                len: NativeStringLen::Immediate(2),
            },
            span,
            overflow_message,
        );
        self.asm.bind_text_label(no_separator);
        self.asm.mov_data_addr(Reg::R10, emitted);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rax);

        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, start);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, end);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);

        self.asm.bind_text_label(copy_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R9, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(copy_loop);

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
    }

    fn emit_runtime_lines_count(&mut self, input: NativeStringRef) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.cmp_reg_imm8(Reg::Rdx, 0);
        let done = self.asm.create_text_label();
        let loop_label = self.asm.create_text_label();
        let next = self.asm.create_text_label();
        let after_loop = self.asm.create_text_label();
        let trailing_newline = self.asm.create_text_label();
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, after_loop);
        self.asm.movzx_byte_indexed(Reg::Rcx, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rcx, b'\n' as i8);
        self.asm.jcc_label(Condition::NotEqual, next);
        self.asm.inc_reg(Reg::Rax);
        self.asm.bind_text_label(next);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(after_loop);
        self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.dec_reg(Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rcx, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rcx, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, trailing_newline);
        self.asm.inc_reg(Reg::Rax);
        self.asm.bind_text_label(trailing_newline);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_lines_head(&mut self, input: NativeStringRef, span: Span) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.cmp_reg_imm8(Reg::Rdx, 0);
        let non_empty = self.asm.create_text_label();
        self.asm.jcc_label(Condition::NotEqual, non_empty);
        self.emit_head_empty(span);
        self.asm.bind_text_label(non_empty);

        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);
        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_lines_tail(&mut self, input: NativeStringRef) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        let find_newline = self.asm.create_text_label();
        let empty = self.asm.create_text_label();
        let start_copy = self.asm.create_text_label();
        let copy_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(find_newline);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, empty);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, start_copy);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(find_newline);

        self.asm.bind_text_label(start_copy);
        self.asm.inc_reg(Reg::R8);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.bind_text_label(copy_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(copy_loop);

        self.asm.bind_text_label(empty);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
        NativeValue::RuntimeLinesList { data, len }
    }

    fn emit_runtime_string_split_static_delimiter(
        &mut self,
        input: NativeStringRef,
        delimiter: String,
        span: Span,
    ) -> Result<NativeValue, Diagnostic> {
        let delimiter = delimiter.into_bytes();
        if delimiter.is_empty() {
            return Ok(self.emit_runtime_string_split_chars(input, span));
        }

        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.mov_imm64(Reg::R10, 1);

        let loop_label = self.asm.create_text_label();
        let no_match = self.asm.create_text_label();
        let write_delimiter = self.asm.create_text_label();
        let write_byte = self.asm.create_text_label();
        let append_trailing_empty = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        let store_len = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);

        for (offset, byte) in delimiter.iter().copied().enumerate() {
            self.asm.mov_reg_reg(Reg::Rcx, Reg::R8);
            if offset > 0 {
                self.asm.add_reg_imm32(Reg::Rcx, offset as i32);
            }
            self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rdx);
            self.asm.jcc_label(Condition::GreaterEqual, no_match);
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rcx);
            self.asm.cmp_reg_imm8(Reg::Rax, byte as i8);
            self.asm.jcc_label(Condition::NotEqual, no_match);
        }
        self.asm.jmp_label(write_delimiter);

        self.asm.bind_text_label(no_match);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.jmp_label(write_byte);

        self.asm.bind_text_label(write_delimiter);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.add_reg_imm32(Reg::R8, delimiter.len() as i32);
        self.asm.inc_reg(Reg::R9);
        self.asm.mov_imm64(Reg::R10, 1);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(write_byte);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.mov_imm64(Reg::R10, 0);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.asm.cmp_reg_imm8(Reg::R10, 0);
        self.asm
            .jcc_label(Condition::NotEqual, append_trailing_empty);
        self.asm.jmp_label(store_len);

        self.asm.bind_text_label(append_trailing_empty);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R9);

        self.asm.bind_text_label(store_len);
        self.asm.mov_data_addr(Reg::Rax, len);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R9);
        Ok(NativeValue::RuntimeLinesList { data, len })
    }

    fn emit_runtime_string_split_runtime_delimiter(
        &mut self,
        input: NativeStringRef,
        delimiter: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        let split_chars = self.asm.create_text_label();
        let split_delimiter = self.asm.create_text_label();
        let final_done = self.asm.create_text_label();

        self.emit_load_native_string_len(Reg::Rax, delimiter.len);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, split_chars);
        self.asm.jmp_label(split_delimiter);

        self.asm.bind_text_label(split_delimiter);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rdi, delimiter.data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.mov_imm64(Reg::R10, 1);

        let loop_label = self.asm.create_text_label();
        let match_loop = self.asm.create_text_label();
        let no_match = self.asm.create_text_label();
        let write_delimiter = self.asm.create_text_label();
        let append_trailing_empty = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        let store_len = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_imm64(Reg::Rcx, 0);

        self.asm.bind_text_label(match_loop);
        self.emit_load_native_string_len(Reg::Rax, delimiter.len);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
        self.asm.jcc_label(Condition::Equal, write_delimiter);
        self.asm.mov_reg_reg(Reg::Rax, Reg::R8);
        self.asm.add_reg_reg(Reg::Rax, Reg::Rcx);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, no_match);
        self.asm.movzx_byte_indexed(Reg::Rbx, Reg::Rdi, Reg::Rcx);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rax);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, no_match);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(match_loop);

        self.asm.bind_text_label(no_match);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.mov_imm64(Reg::R10, 0);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(write_delimiter);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.emit_load_native_string_len(Reg::Rax, delimiter.len);
        self.asm.add_reg_reg(Reg::R8, Reg::Rax);
        self.asm.inc_reg(Reg::R9);
        self.asm.mov_imm64(Reg::R10, 1);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.asm.cmp_reg_imm8(Reg::R10, 0);
        self.asm
            .jcc_label(Condition::NotEqual, append_trailing_empty);
        self.asm.jmp_label(store_len);

        self.asm.bind_text_label(append_trailing_empty);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R9);

        self.asm.bind_text_label(store_len);
        self.asm.mov_data_addr(Reg::Rax, len);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R9);
        self.asm.jmp_label(final_done);

        self.asm.bind_text_label(split_chars);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);

        let char_loop = self.asm.create_text_label();
        let copy_char = self.asm.create_text_label();
        let continuation_loop = self.asm.create_text_label();
        let chars_done = self.asm.create_text_label();

        self.asm.bind_text_label(char_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, chars_done);
        self.asm.cmp_reg_imm8(Reg::R9, 0);
        self.asm.jcc_label(Condition::Equal, copy_char);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R9);

        self.asm.bind_text_label(copy_char);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);

        self.asm.bind_text_label(continuation_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, char_loop);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::R10, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::R10);
        self.asm.mov_imm64(Reg::R10, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, char_loop);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(continuation_loop);

        self.asm.bind_text_label(chars_done);
        self.asm.mov_data_addr(Reg::Rax, len);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R9);

        self.asm.bind_text_label(final_done);
        NativeValue::RuntimeLinesList { data, len }
    }

    fn emit_runtime_string_split_chars(
        &mut self,
        input: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);

        let char_loop = self.asm.create_text_label();
        let copy_char = self.asm.create_text_label();
        let continuation_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(char_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.cmp_reg_imm8(Reg::R9, 0);
        self.asm.jcc_label(Condition::Equal, copy_char);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R9);

        self.asm.bind_text_label(copy_char);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);

        self.asm.bind_text_label(continuation_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, char_loop);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::R10, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::R10);
        self.asm.mov_imm64(Reg::R10, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, char_loop);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "split result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(continuation_loop);

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::Rax, len);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R9);
        NativeValue::RuntimeLinesList { data, len }
    }

    fn emit_runtime_lines_cons(
        &mut self,
        head: NativeStringRef,
        tail: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rsi, head.data);
        self.emit_load_native_string_len(Reg::Rdx, head.len);
        self.asm.mov_imm64(Reg::R8, 0);

        let copy_head = self.asm.create_text_label();
        let maybe_tail = self.asm.create_text_label();
        let copy_tail = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(copy_head);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, maybe_tail);
        self.emit_runtime_buffer_capacity_check(
            Reg::R10,
            RUNTIME_STRING_CAP,
            span,
            "cons result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R10);
        self.asm.jmp_label(copy_head);

        self.asm.bind_text_label(maybe_tail);
        self.asm.mov_data_addr(Reg::Rsi, tail.data);
        self.emit_load_native_string_len(Reg::Rdx, tail.len);
        self.asm.cmp_reg_imm8(Reg::Rdx, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(
            Reg::R10,
            RUNTIME_STRING_CAP,
            span,
            "cons result exceeds 65536 bytes",
        );
        self.asm.mov_imm64(Reg::Rax, b'\n' as u64);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::R10);
        self.asm.mov_imm64(Reg::R8, 0);

        self.asm.bind_text_label(copy_tail);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(
            Reg::R10,
            RUNTIME_STRING_CAP,
            span,
            "cons result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R10);
        self.asm.jmp_label(copy_tail);

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::Rax, len);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R10);
        NativeValue::RuntimeLinesList { data, len }
    }

    fn emit_runtime_lines_equal_static_list(
        &mut self,
        input: NativeStringRef,
        label: ListLabel,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let elements = self
            .static_lists
            .get(label.0)
            .map(|list| list.elements.clone())
            .unwrap_or_default();
        let mut expected_lines = Vec::with_capacity(elements.len());
        for element in elements {
            let StaticValue::StaticString { label, len } = element else {
                return Err(unsupported(
                    span,
                    "native runtime lines list equality for non-string static list",
                ));
            };
            expected_lines.push((label, len));
        }

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);

        if expected_lines.is_empty() {
            self.asm.cmp_reg_imm8(Reg::Rdx, 0);
            self.asm.setcc_al(Condition::Equal);
            self.asm.movzx_rax_al();
            return Ok(());
        }

        let equal = self.asm.create_text_label();
        let not_equal = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.mov_imm64(Reg::R9, 0);

        for (expected_label, expected_len) in expected_lines {
            let scan = self.asm.create_text_label();
            let segment_end = self.asm.create_text_label();
            let byte_loop = self.asm.create_text_label();
            let segment_ok = self.asm.create_text_label();
            let consumed_at_end = self.asm.create_text_label();
            let next = self.asm.create_text_label();

            self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
            self.asm.jcc_label(Condition::GreaterEqual, not_equal);

            self.asm.mov_reg_reg(Reg::R8, Reg::R9);
            self.asm.bind_text_label(scan);
            self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
            self.asm.jcc_label(Condition::Equal, segment_end);
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
            self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
            self.asm.jcc_label(Condition::Equal, segment_end);
            self.asm.inc_reg(Reg::R8);
            self.asm.jmp_label(scan);

            self.asm.bind_text_label(segment_end);
            self.asm.mov_reg_reg(Reg::Rcx, Reg::R8);
            self.asm.sub_reg_reg(Reg::Rcx, Reg::R9);
            self.asm.cmp_reg_imm32(Reg::Rcx, expected_len as i32);
            self.asm.jcc_label(Condition::NotEqual, not_equal);

            self.asm.mov_data_addr(Reg::Rdi, expected_label);
            self.asm.mov_imm64(Reg::Rcx, 0);
            self.asm.bind_text_label(byte_loop);
            self.asm.cmp_reg_imm32(Reg::Rcx, expected_len as i32);
            self.asm.jcc_label(Condition::Equal, segment_ok);
            self.asm.mov_reg_reg(Reg::Rbx, Reg::R9);
            self.asm.add_reg_reg(Reg::Rbx, Reg::Rcx);
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rbx);
            self.asm.movzx_byte_indexed(Reg::Rbx, Reg::Rdi, Reg::Rcx);
            self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
            self.asm.jcc_label(Condition::NotEqual, not_equal);
            self.asm.inc_reg(Reg::Rcx);
            self.asm.jmp_label(byte_loop);

            self.asm.bind_text_label(segment_ok);
            self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
            self.asm.jcc_label(Condition::Equal, consumed_at_end);
            self.asm.mov_reg_reg(Reg::R9, Reg::R8);
            self.asm.inc_reg(Reg::R9);
            self.asm.jmp_label(next);

            self.asm.bind_text_label(consumed_at_end);
            self.asm.mov_reg_reg(Reg::R9, Reg::Rdx);
            self.asm.bind_text_label(next);
        }

        self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, equal);
        self.asm.jmp_label(not_equal);

        self.asm.bind_text_label(equal);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_equal);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done);
        Ok(())
    }

    fn emit_runtime_lines_contains_string(
        &mut self,
        input: NativeStringRef,
        needle: NativeStringRef,
    ) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rdi, needle.data);

        let found = self.asm.create_text_label();
        let not_found = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        let scan = self.asm.create_text_label();
        let segment_end = self.asm.create_text_label();
        let byte_loop = self.asm.create_text_label();
        let advance = self.asm.create_text_label();

        self.asm.cmp_reg_imm8(Reg::Rdx, 0);
        self.asm.jcc_label(Condition::Equal, not_found);
        self.asm.mov_imm64(Reg::R9, 0);

        self.asm.bind_text_label(scan);
        self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, not_found);
        self.asm.mov_reg_reg(Reg::R8, Reg::R9);

        let find_segment_end = self.asm.create_text_label();
        self.asm.bind_text_label(find_segment_end);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, segment_end);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, segment_end);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(find_segment_end);

        self.asm.bind_text_label(segment_end);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::R8);
        self.asm.sub_reg_reg(Reg::Rcx, Reg::R9);
        self.emit_load_native_string_len(Reg::Rax, needle.len);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
        self.asm.jcc_label(Condition::NotEqual, advance);

        self.asm.mov_imm64(Reg::Rcx, 0);
        self.asm.bind_text_label(byte_loop);
        self.emit_load_native_string_len(Reg::Rax, needle.len);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
        self.asm.jcc_label(Condition::Equal, found);
        self.asm.mov_reg_reg(Reg::Rbx, Reg::R9);
        self.asm.add_reg_reg(Reg::Rbx, Reg::Rcx);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rbx);
        self.asm.movzx_byte_indexed(Reg::Rbx, Reg::Rdi, Reg::Rcx);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, advance);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(byte_loop);

        self.asm.bind_text_label(advance);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, not_found);
        self.asm.mov_reg_reg(Reg::R9, Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(found);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_found);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_lines_equal_runtime_lines(
        &mut self,
        lhs: NativeStringRef,
        rhs: NativeStringRef,
    ) {
        let lhs_start = self.asm.data_label_with_i64s(&[0]);
        let rhs_start = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rax, lhs_start);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::Rax, rhs_start);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);

        self.asm.mov_data_addr(Reg::Rsi, lhs.data);
        self.emit_load_native_string_len(Reg::Rdx, lhs.len);
        self.asm.mov_data_addr(Reg::Rdi, rhs.data);
        self.emit_load_native_string_len(Reg::R10, rhs.len);

        let loop_label = self.asm.create_text_label();
        let lhs_at_end = self.asm.create_text_label();
        let scan_lhs = self.asm.create_text_label();
        let lhs_segment_end = self.asm.create_text_label();
        let scan_rhs = self.asm.create_text_label();
        let rhs_segment_end = self.asm.create_text_label();
        let byte_loop = self.asm.create_text_label();
        let segment_ok = self.asm.create_text_label();
        let lhs_consumed_at_end = self.asm.create_text_label();
        let rhs_consumed_at_end = self.asm.create_text_label();
        let store_rhs_start = self.asm.create_text_label();
        let equal = self.asm.create_text_label();
        let not_equal = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.mov_data_addr(Reg::Rax, lhs_start);
        self.asm.load_ptr_disp32(Reg::R8, Reg::Rax, 0);
        self.asm.mov_data_addr(Reg::Rax, rhs_start);
        self.asm.load_ptr_disp32(Reg::R9, Reg::Rax, 0);

        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, lhs_at_end);
        self.asm.jcc_label(Condition::Greater, not_equal);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::GreaterEqual, not_equal);
        self.asm.jmp_label(scan_lhs);

        self.asm.bind_text_label(lhs_at_end);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::Equal, equal);
        self.asm.jmp_label(not_equal);

        self.asm.bind_text_label(scan_lhs);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, lhs_segment_end);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, lhs_segment_end);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(scan_lhs);

        self.asm.bind_text_label(lhs_segment_end);
        self.asm.bind_text_label(scan_rhs);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::Equal, rhs_segment_end);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rdi, Reg::R9);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, rhs_segment_end);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(scan_rhs);

        self.asm.bind_text_label(rhs_segment_end);
        self.asm.mov_data_addr(Reg::Rax, lhs_start);
        self.asm.load_ptr_disp32(Reg::Rbx, Reg::Rax, 0);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::R8);
        self.asm.sub_reg_reg(Reg::Rcx, Reg::Rbx);
        self.asm.mov_data_addr(Reg::Rax, rhs_start);
        self.asm.load_ptr_disp32(Reg::Rbx, Reg::Rax, 0);
        self.asm.mov_reg_reg(Reg::Rax, Reg::R9);
        self.asm.sub_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
        self.asm.jcc_label(Condition::NotEqual, not_equal);

        self.asm.mov_reg_reg(Reg::R10, Reg::Rcx);
        self.asm.mov_imm64(Reg::Rcx, 0);
        self.asm.bind_text_label(byte_loop);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R10);
        self.asm.jcc_label(Condition::Equal, segment_ok);
        self.asm.mov_data_addr(Reg::Rax, lhs_start);
        self.asm.load_ptr_disp32(Reg::Rbx, Reg::Rax, 0);
        self.asm.add_reg_reg(Reg::Rbx, Reg::Rcx);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rbx);
        self.asm.mov_data_addr(Reg::Rbx, rhs_start);
        self.asm.load_ptr_disp32(Reg::Rbx, Reg::Rbx, 0);
        self.asm.add_reg_reg(Reg::Rbx, Reg::Rcx);
        self.asm.movzx_byte_indexed(Reg::Rbx, Reg::Rdi, Reg::Rbx);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, not_equal);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(byte_loop);

        self.asm.bind_text_label(segment_ok);
        self.emit_load_native_string_len(Reg::R10, rhs.len);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, lhs_consumed_at_end);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(rhs_consumed_at_end);

        self.asm.bind_text_label(lhs_consumed_at_end);
        self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);

        self.asm.bind_text_label(rhs_consumed_at_end);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::Equal, store_rhs_start);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(store_rhs_start);

        self.asm.bind_text_label(store_rhs_start);
        self.asm.mov_data_addr(Reg::Rax, lhs_start);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R8);
        self.asm.mov_data_addr(Reg::Rax, rhs_start);
        self.asm.store_ptr_disp32(Reg::Rax, 0, Reg::R9);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(equal);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_equal);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_lines_join(
        &mut self,
        input: NativeStringRef,
        delimiter: DataLabel,
        delimiter_len: usize,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.mov_imm64(Reg::R10, 0);

        let scan = self.asm.create_text_label();
        let emit_line = self.asm.create_text_label();
        let finish = self.asm.create_text_label();
        let emit_final = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(scan);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, finish);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, emit_line);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(emit_line);
        self.emit_runtime_lines_join_segment(
            delimiter,
            delimiter_len,
            span,
            "join result exceeds 65536 bytes",
        );
        self.asm.inc_reg(Reg::R8);
        self.asm.mov_reg_reg(Reg::R9, Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(finish);
        self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.jcc_label(Condition::Less, emit_final);
        self.asm.jmp_label(done);

        self.asm.bind_text_label(emit_final);
        self.emit_runtime_lines_join_segment(
            delimiter,
            delimiter_len,
            span,
            "join result exceeds 65536 bytes",
        );

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::Rcx, len);
        self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::R10);
        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_lines_join_runtime_delimiter(
        &mut self,
        input: NativeStringRef,
        delimiter: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.mov_imm64(Reg::R10, 0);

        let scan = self.asm.create_text_label();
        let emit_line = self.asm.create_text_label();
        let finish = self.asm.create_text_label();
        let emit_final = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(scan);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, finish);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'\n' as i8);
        self.asm.jcc_label(Condition::Equal, emit_line);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(emit_line);
        self.emit_runtime_lines_join_runtime_delimiter_segment(
            delimiter,
            span,
            "join result exceeds 65536 bytes",
        );
        self.asm.inc_reg(Reg::R8);
        self.asm.mov_reg_reg(Reg::R9, Reg::R8);
        self.asm.jmp_label(scan);

        self.asm.bind_text_label(finish);
        self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.jcc_label(Condition::Less, emit_final);
        self.asm.jmp_label(done);

        self.asm.bind_text_label(emit_final);
        self.emit_runtime_lines_join_runtime_delimiter_segment(
            delimiter,
            span,
            "join result exceeds 65536 bytes",
        );

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::Rcx, len);
        self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::R10);
        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_lines_join_segment(
        &mut self,
        delimiter: DataLabel,
        delimiter_len: usize,
        span: Span,
        overflow_message: &str,
    ) {
        let copy_segment = self.asm.create_text_label();
        let delimiter_loop = self.asm.create_text_label();
        let segment_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.cmp_reg_imm8(Reg::R9, 0);
        self.asm.jcc_label(Condition::Equal, copy_segment);
        self.asm.mov_data_addr(Reg::Rdi, delimiter);
        self.asm.mov_imm64(Reg::Rcx, 0);
        self.asm.bind_text_label(delimiter_loop);
        self.asm.cmp_reg_imm32(Reg::Rcx, delimiter_len as i32);
        self.asm.jcc_label(Condition::Equal, copy_segment);
        self.emit_runtime_buffer_capacity_check(Reg::R10, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rdi, Reg::Rcx);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.inc_reg(Reg::R10);
        self.asm.jmp_label(delimiter_loop);

        self.asm.bind_text_label(copy_segment);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::R9);
        self.asm.bind_text_label(segment_loop);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R8);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R10, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rcx);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.inc_reg(Reg::R10);
        self.asm.jmp_label(segment_loop);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_lines_join_runtime_delimiter_segment(
        &mut self,
        delimiter: NativeStringRef,
        span: Span,
        overflow_message: &str,
    ) {
        let copy_segment = self.asm.create_text_label();
        let delimiter_loop = self.asm.create_text_label();
        let segment_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.cmp_reg_imm8(Reg::R9, 0);
        self.asm.jcc_label(Condition::Equal, copy_segment);
        self.asm.mov_data_addr(Reg::Rdi, delimiter.data);
        self.asm.mov_imm64(Reg::Rcx, 0);
        self.asm.bind_text_label(delimiter_loop);
        self.emit_load_native_string_len(Reg::Rax, delimiter.len);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::Rax);
        self.asm.jcc_label(Condition::Equal, copy_segment);
        self.emit_runtime_buffer_capacity_check(Reg::R10, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rdi, Reg::Rcx);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.inc_reg(Reg::R10);
        self.asm.jmp_label(delimiter_loop);

        self.asm.bind_text_label(copy_segment);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::R9);
        self.asm.bind_text_label(segment_loop);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R8);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R10, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rcx);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.inc_reg(Reg::R10);
        self.asm.jmp_label(segment_loop);
        self.asm.bind_text_label(done);
    }

    fn emit_print_static_record(&mut self, fd: u64, label: RecordLabel) {
        let Some(record) = self.static_records.get(label.0).cloned() else {
            return;
        };
        self.emit_write_data(fd, self.hash, 1);
        if !record.name.is_empty() {
            let name = self.asm.data_label_with_bytes(record.name.as_bytes());
            self.emit_write_data(fd, name, record.name.len());
        }
        self.emit_write_data(fd, self.paren_open, 1);
        for (index, (_, value)) in record.fields.iter().enumerate() {
            if index > 0 {
                self.emit_write_data(fd, self.comma_space, 2);
            }
            let value = self.emit_static_value(value);
            self.emit_print_value_fragment(fd, value);
        }
        self.emit_write_data(fd, self.paren_close, 1);
    }

    fn emit_print_static_map(&mut self, fd: u64, label: MapLabel) {
        let Some(map) = self.static_maps.get(label.0).cloned() else {
            return;
        };
        self.emit_write_data(fd, self.map_open, 2);
        for (index, (key, value)) in map.entries.iter().enumerate() {
            if index > 0 {
                self.emit_write_data(fd, self.comma_space, 2);
            }
            let key = self.emit_static_value(key);
            self.emit_print_value_fragment(fd, key);
            self.emit_write_data(fd, self.colon_space, 2);
            let value = self.emit_static_value(value);
            self.emit_print_value_fragment(fd, value);
        }
        self.emit_write_data(fd, self.list_close, 1);
    }

    fn emit_print_static_set(&mut self, fd: u64, label: SetLabel) {
        let Some(set) = self.static_sets.get(label.0).cloned() else {
            return;
        };
        self.emit_write_data(fd, self.set_open, 2);
        for (index, value) in set.elements.iter().enumerate() {
            if index > 0 {
                self.emit_write_data(fd, self.comma_space, 2);
            }
            let value = self.emit_static_value(value);
            self.emit_print_value_fragment(fd, value);
        }
        self.emit_write_data(fd, self.paren_close, 1);
    }

    fn emit_print_bool_fragment(&mut self, fd: u64) {
        let false_label = self.asm.create_text_label();
        let end_label = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, false_label);
        self.emit_write_data(fd, self.true_text, 4);
        self.asm.jmp_label(end_label);
        self.asm.bind_text_label(false_label);
        self.emit_write_data(fd, self.false_text, 5);
        self.asm.bind_text_label(end_label);
    }

    fn emit_write_data(&mut self, fd: u64, label: DataLabel, len: usize) {
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.mov_imm64(Reg::Rdi, fd);
        self.asm.mov_data_addr(Reg::Rsi, label);
        self.asm.mov_imm64(Reg::Rdx, len as u64);
        self.asm.syscall();
    }

    fn emit_write_data_dynamic_len(&mut self, fd: u64, data: DataLabel, len: DataLabel) {
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.mov_imm64(Reg::Rdi, fd);
        self.asm.mov_data_addr(Reg::Rsi, data);
        self.asm.mov_data_addr(Reg::Rdx, len);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::Rdx, 0);
        self.asm.syscall();
    }

    fn native_string_ref(&self, value: NativeValue) -> Option<NativeStringRef> {
        match value {
            NativeValue::StaticString { label, len } => Some(NativeStringRef {
                data: label,
                len: NativeStringLen::Immediate(len),
            }),
            NativeValue::RuntimeString { data, len } => Some(NativeStringRef {
                data,
                len: NativeStringLen::Runtime(len),
            }),
            _ => None,
        }
    }

    fn emit_load_native_string_len(&mut self, dst: Reg, len: NativeStringLen) {
        match len {
            NativeStringLen::Immediate(value) => self.asm.mov_imm64(dst, value as u64),
            NativeStringLen::Runtime(label) => {
                self.asm.mov_data_addr(dst, label);
                self.asm.load_ptr_disp32(dst, dst, 0);
            }
        }
    }

    fn emit_native_string_equality(&mut self, lhs: NativeStringRef, rhs: NativeStringRef) {
        self.asm.mov_data_addr(Reg::Rsi, lhs.data);
        self.emit_load_native_string_len(Reg::Rdx, lhs.len);
        self.asm.mov_data_addr(Reg::Rdi, rhs.data);
        self.emit_load_native_string_len(Reg::Rcx, rhs.len);

        let loop_label = self.asm.create_text_label();
        let equal_label = self.asm.create_text_label();
        let not_equal_label = self.asm.create_text_label();
        let done_label = self.asm.create_text_label();

        self.asm.cmp_reg_reg(Reg::Rdx, Reg::Rcx);
        self.asm.jcc_label(Condition::NotEqual, not_equal_label);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, equal_label);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::R10, Reg::Rdi, Reg::R8);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, not_equal_label);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(equal_label);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done_label);
        self.asm.bind_text_label(not_equal_label);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done_label);
    }

    fn emit_runtime_string_char_length(&mut self, value: NativeStringRef) {
        self.asm.mov_data_addr(Reg::Rsi, value.data);
        self.emit_load_native_string_len(Reg::Rdx, value.len);

        let loop_label = self.asm.create_text_label();
        let skip_count = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::R10, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::R10);
        self.asm.mov_imm64(Reg::R10, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::Equal, skip_count);
        self.asm.inc_reg(Reg::R9);
        self.asm.bind_text_label(skip_count);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
        self.asm.mov_reg_reg(Reg::Rax, Reg::R9);
    }

    fn emit_native_string_starts_with(&mut self, input: NativeStringRef, needle: NativeStringRef) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rdi, needle.data);
        self.emit_load_native_string_len(Reg::Rcx, needle.len);
        let length_ok = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.cmp_reg_reg(Reg::Rdx, Reg::Rcx);
        self.asm.jcc_label(Condition::GreaterEqual, length_ok);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(length_ok);
        self.asm.mov_imm64(Reg::R9, 0);
        self.emit_native_string_match_at_current_offset_with_done(done);
    }

    fn emit_native_string_ends_with(&mut self, input: NativeStringRef, needle: NativeStringRef) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rdi, needle.data);
        self.emit_load_native_string_len(Reg::Rcx, needle.len);
        let length_ok = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.cmp_reg_reg(Reg::Rdx, Reg::Rcx);
        self.asm.jcc_label(Condition::GreaterEqual, length_ok);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(length_ok);
        self.asm.mov_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.sub_reg_reg(Reg::R9, Reg::Rcx);
        self.emit_native_string_match_at_current_offset_with_done(done);
    }

    fn emit_native_string_contains(&mut self, input: NativeStringRef, needle: NativeStringRef) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rdi, needle.data);
        self.emit_load_native_string_len(Reg::Rcx, needle.len);

        let outer_loop = self.asm.create_text_label();
        let inner_loop = self.asm.create_text_label();
        let next_offset = self.asm.create_text_label();
        let found = self.asm.create_text_label();
        let not_found = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.cmp_reg_imm8(Reg::Rcx, 0);
        self.asm.jcc_label(Condition::Equal, found);
        self.asm.cmp_reg_reg(Reg::Rdx, Reg::Rcx);
        self.asm.jcc_label(Condition::Less, not_found);
        self.asm.mov_imm64(Reg::R9, 0);

        self.asm.bind_text_label(outer_loop);
        self.asm.mov_reg_reg(Reg::Rbx, Reg::R9);
        self.asm.add_reg_reg(Reg::Rbx, Reg::Rcx);
        self.asm.cmp_reg_reg(Reg::Rdx, Reg::Rbx);
        self.asm.jcc_label(Condition::Less, not_found);
        self.asm.mov_imm64(Reg::R8, 0);

        self.asm.bind_text_label(inner_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, found);
        self.asm.mov_reg_reg(Reg::Rbx, Reg::R9);
        self.asm.add_reg_reg(Reg::Rbx, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rbx);
        self.asm.movzx_byte_indexed(Reg::R10, Reg::Rdi, Reg::R8);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, next_offset);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(inner_loop);

        self.asm.bind_text_label(next_offset);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(outer_loop);

        self.asm.bind_text_label(found);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_found);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done);
    }

    fn emit_native_string_index_of(
        &mut self,
        input: NativeStringRef,
        needle: NativeStringRef,
        find_last: bool,
    ) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rdi, needle.data);
        self.emit_load_native_string_len(Reg::Rcx, needle.len);

        let search = self.asm.create_text_label();
        let inner_loop = self.asm.create_text_label();
        let next_offset = self.asm.create_text_label();
        let found = self.asm.create_text_label();
        let not_found = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.cmp_reg_imm8(Reg::Rcx, 0);
        if find_last {
            let non_empty = self.asm.create_text_label();
            self.asm.jcc_label(Condition::NotEqual, non_empty);
            self.asm.mov_reg_reg(Reg::Rax, Reg::Rdx);
            self.asm.jmp_label(done);
            self.asm.bind_text_label(non_empty);
        } else {
            self.asm.jcc_label(Condition::Equal, found);
        }
        self.asm.cmp_reg_reg(Reg::Rdx, Reg::Rcx);
        self.asm.jcc_label(Condition::Less, not_found);
        if find_last {
            self.asm.mov_reg_reg(Reg::R9, Reg::Rdx);
            self.asm.sub_reg_reg(Reg::R9, Reg::Rcx);
        } else {
            self.asm.mov_imm64(Reg::R9, 0);
        }

        self.asm.bind_text_label(search);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(inner_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, found);
        self.asm.mov_reg_reg(Reg::Rbx, Reg::R9);
        self.asm.add_reg_reg(Reg::Rbx, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rbx);
        self.asm.movzx_byte_indexed(Reg::R10, Reg::Rdi, Reg::R8);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, next_offset);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(inner_loop);

        self.asm.bind_text_label(next_offset);
        if find_last {
            self.asm.cmp_reg_imm8(Reg::R9, 0);
            self.asm.jcc_label(Condition::Equal, not_found);
            self.asm.dec_reg(Reg::R9);
        } else {
            self.asm.inc_reg(Reg::R9);
            self.asm.mov_reg_reg(Reg::Rbx, Reg::R9);
            self.asm.add_reg_reg(Reg::Rbx, Reg::Rcx);
            self.asm.cmp_reg_reg(Reg::Rdx, Reg::Rbx);
            self.asm.jcc_label(Condition::Less, not_found);
        }
        self.asm.jmp_label(search);

        self.asm.bind_text_label(found);
        self.asm.mov_reg_reg(Reg::Rax, Reg::R9);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_found);
        self.asm.mov_imm64(Reg::Rax, (-1i64) as u64);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_string_slice(
        &mut self,
        input: NativeStringRef,
        start_chars: usize,
        end_chars: usize,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let start_byte = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.emit_native_string_byte_offset_for_char_index(start_chars);
        self.asm.mov_data_addr(Reg::R10, start_byte);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.emit_native_string_byte_offset_for_char_index(end_chars);
        self.asm.mov_data_addr(Reg::R10, start_byte);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::R8);
        self.asm.sub_reg_reg(Reg::Rcx, Reg::R9);
        self.asm.mov_imm64(Reg::R10, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R10);
        let copy = self.asm.create_text_label();
        self.asm.jcc_label(Condition::LessEqual, copy);
        self.emit_runtime_error(span, "substring result exceeds 65536 bytes");

        self.asm.bind_text_label(copy);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rcx);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_reg_reg(Reg::R10, Reg::R9);
        self.asm.add_reg_reg(Reg::R10, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_slice_dynamic_indices(
        &mut self,
        input: NativeStringRef,
        start_chars: DataLabel,
        end_chars: DataLabel,
        span: Span,
        name: &str,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let start_byte = self.asm.data_label_with_i64s(&[0]);
        let effective_end_chars = self.asm.data_label_with_i64s(&[0]);
        let non_negative = self.asm.create_text_label();
        let end_non_negative = self.asm.create_text_label();
        let store_effective_end = self.asm.create_text_label();
        let error_message = format!("{name} expects a non-negative integer index");

        self.asm.mov_data_addr(Reg::R10, start_chars);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::R8, 0);
        self.asm.jcc_label(Condition::GreaterEqual, non_negative);
        self.emit_runtime_error(span, &error_message);

        self.asm.bind_text_label(non_negative);
        self.asm.mov_data_addr(Reg::R10, end_chars);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::R9, 0);
        self.asm
            .jcc_label(Condition::GreaterEqual, end_non_negative);
        self.emit_runtime_error(span, &error_message);

        self.asm.bind_text_label(end_non_negative);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R8);
        self.asm
            .jcc_label(Condition::GreaterEqual, store_effective_end);
        self.asm.mov_reg_reg(Reg::R9, Reg::R8);
        self.asm.bind_text_label(store_effective_end);
        self.asm.mov_data_addr(Reg::R10, effective_end_chars);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.emit_native_string_byte_offset_for_char_index_dynamic(start_chars);
        self.asm.mov_data_addr(Reg::R10, start_byte);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R8);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.emit_native_string_byte_offset_for_char_index_dynamic(effective_end_chars);
        self.asm.mov_data_addr(Reg::R10, start_byte);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::R8);
        self.asm.sub_reg_reg(Reg::Rcx, Reg::R9);
        self.asm.mov_imm64(Reg::R10, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R10);
        let copy = self.asm.create_text_label();
        self.asm.jcc_label(Condition::LessEqual, copy);
        self.emit_runtime_error(span, "substring result exceeds 65536 bytes");

        self.asm.bind_text_label(copy);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rcx);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_reg_reg(Reg::R10, Reg::R9);
        self.asm.add_reg_reg(Reg::R10, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_native_string_byte_offset_for_char_index(&mut self, char_index: usize) {
        let loop_label = self.asm.create_text_label();
        let consume = self.asm.create_text_label();
        let skip_count = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.mov_imm64(Reg::R10, char_index as u64);
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, done);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, consume);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::Rbx, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.mov_imm64(Reg::Rbx, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, done);

        self.asm.bind_text_label(consume);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::Rbx, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.mov_imm64(Reg::Rbx, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::Equal, skip_count);
        self.asm.inc_reg(Reg::R9);
        self.asm.bind_text_label(skip_count);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
    }

    fn emit_native_string_byte_offset_for_char_index_dynamic(&mut self, char_index: DataLabel) {
        let loop_label = self.asm.create_text_label();
        let consume = self.asm.create_text_label();
        let skip_count = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.mov_data_addr(Reg::R10, char_index);
        self.asm.load_ptr_disp32(Reg::R10, Reg::R10, 0);
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::GreaterEqual, done);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, consume);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::Rbx, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.mov_imm64(Reg::Rbx, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, done);

        self.asm.bind_text_label(consume);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::Rbx, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.mov_imm64(Reg::Rbx, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::Equal, skip_count);
        self.asm.inc_reg(Reg::R9);
        self.asm.bind_text_label(skip_count);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_string_trim(
        &mut self,
        input: NativeStringRef,
        trim_left: bool,
        trim_right: bool,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let start_byte = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);

        if trim_left {
            let left_loop = self.asm.create_text_label();
            let consume = self.asm.create_text_label();
            let left_done = self.asm.create_text_label();
            self.asm.mov_imm64(Reg::R9, 0);
            self.asm.bind_text_label(left_loop);
            self.asm.cmp_reg_reg(Reg::R9, Reg::Rdx);
            self.asm.jcc_label(Condition::GreaterEqual, left_done);
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R9);
            self.emit_jump_if_ascii_whitespace(Reg::Rax, consume);
            self.asm.jmp_label(left_done);
            self.asm.bind_text_label(consume);
            self.asm.inc_reg(Reg::R9);
            self.asm.jmp_label(left_loop);
            self.asm.bind_text_label(left_done);
        } else {
            self.asm.mov_imm64(Reg::R9, 0);
        }
        self.asm.mov_data_addr(Reg::R10, start_byte);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        if trim_right {
            let right_loop = self.asm.create_text_label();
            let trim_more = self.asm.create_text_label();
            let right_done = self.asm.create_text_label();
            self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);
            self.asm.bind_text_label(right_loop);
            self.asm.cmp_reg_reg(Reg::R8, Reg::R9);
            self.asm.jcc_label(Condition::LessEqual, right_done);
            self.asm.dec_reg(Reg::R8);
            self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
            self.emit_jump_if_ascii_whitespace(Reg::Rax, trim_more);
            self.asm.inc_reg(Reg::R8);
            self.asm.jmp_label(right_done);
            self.asm.bind_text_label(trim_more);
            self.asm.jmp_label(right_loop);
            self.asm.bind_text_label(right_done);
        } else {
            self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);
        }

        self.asm.mov_data_addr(Reg::R10, start_byte);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::R8);
        self.asm.sub_reg_reg(Reg::Rcx, Reg::R9);
        self.asm.mov_imm64(Reg::R10, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R10);
        let copy = self.asm.create_text_label();
        self.asm.jcc_label(Condition::LessEqual, copy);
        self.emit_runtime_error(span, "trim result exceeds 65536 bytes");

        self.asm.bind_text_label(copy);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rcx);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_reg_reg(Reg::R10, Reg::R9);
        self.asm.add_reg_reg(Reg::R10, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_repeat(
        &mut self,
        input: NativeStringRef,
        count: usize,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::Rdx);
        self.asm.mov_imm64(Reg::R9, count as u64);
        self.asm.imul_reg_reg(Reg::Rcx, Reg::R9);
        self.asm.mov_imm64(Reg::R10, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R10);
        let copy = self.asm.create_text_label();
        self.asm.jcc_label(Condition::LessEqual, copy);
        self.emit_runtime_error(span, "repeat result exceeds 65536 bytes");

        self.asm.bind_text_label(copy);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rcx);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::Rdi, count as u64);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        let outer_loop = self.asm.create_text_label();
        let inner_loop = self.asm.create_text_label();
        let next_repeat = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(outer_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdi);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_imm64(Reg::R10, 0);
        self.asm.bind_text_label(inner_loop);
        self.asm.cmp_reg_reg(Reg::R10, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, next_repeat);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R10);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(inner_loop);
        self.asm.bind_text_label(next_repeat);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(outer_loop);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_repeat_dynamic_count(
        &mut self,
        input: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let count_slot = self.asm.data_label_with_i64s(&[0]);

        let count_ok = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::GreaterEqual, count_ok);
        self.emit_runtime_error(span, "repeat expects a non-negative integer index");
        self.asm.bind_text_label(count_ok);

        self.asm.mov_data_addr(Reg::Rcx, count_slot);
        self.asm.store_ptr_disp32(Reg::Rcx, 0, Reg::Rax);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_reg_reg(Reg::Rcx, Reg::Rdx);
        self.asm.mov_data_addr(Reg::R9, count_slot);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R9, 0);
        self.asm.imul_reg_reg(Reg::Rcx, Reg::R9);
        self.asm.mov_imm64(Reg::R10, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::Rcx, Reg::R10);
        let copy = self.asm.create_text_label();
        self.asm.jcc_label(Condition::LessEqual, copy);
        self.emit_runtime_error(span, "repeat result exceeds 65536 bytes");

        self.asm.bind_text_label(copy);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rcx);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_data_addr(Reg::Rdi, count_slot);
        self.asm.load_ptr_disp32(Reg::Rdi, Reg::Rdi, 0);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        let outer_loop = self.asm.create_text_label();
        let inner_loop = self.asm.create_text_label();
        let next_repeat = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(outer_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdi);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_imm64(Reg::R10, 0);
        self.asm.bind_text_label(inner_loop);
        self.asm.cmp_reg_reg(Reg::R10, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, next_repeat);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R10);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(inner_loop);
        self.asm.bind_text_label(next_repeat);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(outer_loop);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_ascii_case(
        &mut self,
        input: NativeStringRef,
        to_upper: bool,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rdx);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let maybe_convert = self.asm.create_text_label();
        let store = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        if to_upper {
            self.asm.cmp_reg_imm8(Reg::Rax, b'a' as i8);
        } else {
            self.asm.cmp_reg_imm8(Reg::Rax, b'A' as i8);
        }
        self.asm.jcc_label(Condition::GreaterEqual, maybe_convert);
        self.asm.jmp_label(store);
        self.asm.bind_text_label(maybe_convert);
        if to_upper {
            self.asm.cmp_reg_imm8(Reg::Rax, b'z' as i8);
            self.asm.jcc_label(Condition::Greater, store);
            self.asm.add_reg8_imm8(Reg8::Al, 224);
        } else {
            self.asm.cmp_reg_imm8(Reg::Rax, b'Z' as i8);
            self.asm.jcc_label(Condition::Greater, store);
            self.asm.add_reg8_imm8(Reg8::Al, 32);
        }
        self.asm.bind_text_label(store);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_replace_first_dynamic(
        &mut self,
        input: NativeStringRef,
        from: NativeStringRef,
        to: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let found_offset = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);

        self.emit_native_string_index_of(input, from, false);
        self.asm.mov_data_addr(Reg::R10, found_offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rax);

        let found = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.mov_data_addr(Reg::R10, found_offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.cmp_reg_imm8(Reg::R9, -1);
        self.asm.jcc_label(Condition::NotEqual, found);

        self.emit_append_native_string_to_runtime_buffer_offset_label(
            data,
            offset,
            input,
            span,
            "replace result exceeds 65536 bytes",
        );
        self.asm.jmp_label(done);

        self.asm.bind_text_label(found);
        self.emit_copy_input_prefix_to_runtime_buffer(input, data, found_offset);
        self.asm.mov_data_addr(Reg::R10, found_offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            data,
            offset,
            to,
            span,
            "replace result exceeds 65536 bytes",
        );
        self.emit_append_input_suffix_after_dynamic_replacement(
            input,
            data,
            found_offset,
            from,
            offset,
            span,
            "replace result exceeds 65536 bytes",
        );

        self.asm.bind_text_label(done);
        self.emit_store_runtime_string_len_from_offset(len, offset);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_replace_all_static_pattern(
        &mut self,
        input: NativeStringRef,
        pattern: String,
        replacement: String,
        span: Span,
    ) -> NativeValue {
        if pattern == "[0-9]" {
            self.emit_runtime_string_replace_all_ascii_digits(input, replacement, span)
        } else if pattern.is_empty() {
            self.emit_runtime_string_replace_all_empty_pattern(input, replacement, span)
        } else {
            self.emit_runtime_string_replace_all_literal(input, pattern, replacement, span)
        }
    }

    fn emit_runtime_string_replace_all_ascii_digits(
        &mut self,
        input: NativeStringRef,
        replacement: String,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let replacement_label = self.asm.data_label_with_bytes(replacement.as_bytes());
        let replacement_len = replacement.len();

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);

        let loop_label = self.asm.create_text_label();
        let replace = self.asm.create_text_label();
        let copy = self.asm.create_text_label();
        let after_write = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'0' as i8);
        self.asm.jcc_label(Condition::Less, copy);
        self.asm.cmp_reg_imm8(Reg::Rax, b'9' as i8);
        self.asm.jcc_label(Condition::Greater, copy);

        self.asm.bind_text_label(replace);
        self.emit_append_static_string_to_runtime_buffer_dynamic(
            Reg::Rbx,
            Reg::R9,
            replacement_label,
            replacement_len,
            span,
            "replaceAll result exceeds 65536 bytes",
        );
        self.asm.jmp_label(after_write);

        self.asm.bind_text_label(copy);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "replaceAll result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R9);

        self.asm.bind_text_label(after_write);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_replace_all_empty_pattern(
        &mut self,
        input: NativeStringRef,
        replacement: String,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let replacement_label = self.asm.data_label_with_bytes(replacement.as_bytes());
        let replacement_len = replacement.len();

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);
        self.emit_append_static_string_to_runtime_buffer_dynamic(
            Reg::Rbx,
            Reg::R9,
            replacement_label,
            replacement_len,
            span,
            "replaceAll result exceeds 65536 bytes",
        );

        let loop_label = self.asm.create_text_label();
        let append_separator = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "replaceAll result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, append_separator);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_imm64(Reg::R10, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::R10);
        self.asm.mov_imm64(Reg::R10, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::Equal, loop_label);

        self.asm.bind_text_label(append_separator);
        self.emit_append_static_string_to_runtime_buffer_dynamic(
            Reg::Rbx,
            Reg::R9,
            replacement_label,
            replacement_len,
            span,
            "replaceAll result exceeds 65536 bytes",
        );
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_replace_all_literal(
        &mut self,
        input: NativeStringRef,
        pattern: String,
        replacement: String,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let pattern_label = self.asm.data_label_with_bytes(pattern.as_bytes());
        let replacement_label = self.asm.data_label_with_bytes(replacement.as_bytes());
        let pattern_len = pattern.len();
        let replacement_len = replacement.len();

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rdi, pattern_label);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.mov_imm64(Reg::R9, 0);

        let loop_label = self.asm.create_text_label();
        let try_match = self.asm.create_text_label();
        let match_loop = self.asm.create_text_label();
        let matched = self.asm.create_text_label();
        let copy_one = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_reg_reg(Reg::R10, Reg::R8);
        self.asm.add_reg_imm32(Reg::R10, pattern_len as i32);
        self.asm.cmp_reg_reg(Reg::R10, Reg::Rdx);
        self.asm.jcc_label(Condition::LessEqual, try_match);
        self.asm.jmp_label(copy_one);

        self.asm.bind_text_label(try_match);
        self.asm.mov_imm64(Reg::Rcx, 0);
        self.asm.bind_text_label(match_loop);
        self.asm.cmp_reg_imm32(Reg::Rcx, pattern_len as i32);
        self.asm.jcc_label(Condition::Equal, matched);
        self.asm.mov_reg_reg(Reg::R10, Reg::R8);
        self.asm.add_reg_reg(Reg::R10, Reg::Rcx);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.movzx_byte_indexed(Reg::R10, Reg::Rdi, Reg::Rcx);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::R10);
        self.asm.jcc_label(Condition::NotEqual, copy_one);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(match_loop);

        self.asm.bind_text_label(matched);
        self.emit_append_static_string_to_runtime_buffer_dynamic(
            Reg::Rbx,
            Reg::R9,
            replacement_label,
            replacement_len,
            span,
            "replaceAll result exceeds 65536 bytes",
        );
        self.asm.add_reg_imm32(Reg::R8, pattern_len as i32);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(copy_one);
        self.emit_runtime_buffer_capacity_check(
            Reg::R9,
            RUNTIME_STRING_CAP,
            span,
            "replaceAll result exceeds 65536 bytes",
        );
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_runtime_string_matches_pattern(
        &mut self,
        input: NativeStringRef,
        pattern: NativeStringRef,
    ) {
        let dot_star = NativeStringRef {
            data: self.asm.data_label_with_bytes(b".*"),
            len: NativeStringLen::Immediate(2),
        };
        let digit_plus = NativeStringRef {
            data: self.asm.data_label_with_bytes(b"[0-9]+"),
            len: NativeStringLen::Immediate(6),
        };
        let digit_one = NativeStringRef {
            data: self.asm.data_label_with_bytes(b"[0-9]"),
            len: NativeStringLen::Immediate(5),
        };
        let match_any = self.asm.create_text_label();
        let match_digits = self.asm.create_text_label();
        let match_digit = self.asm.create_text_label();
        let fallback = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.emit_native_string_equality(pattern, dot_star);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::NotEqual, match_any);
        self.emit_native_string_equality(pattern, digit_plus);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::NotEqual, match_digits);
        self.emit_native_string_equality(pattern, digit_one);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::NotEqual, match_digit);
        self.asm.jmp_label(fallback);

        self.asm.bind_text_label(match_any);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done);

        self.asm.bind_text_label(match_digits);
        self.emit_runtime_string_ascii_digits_match(input, false);
        self.asm.jmp_label(done);

        self.asm.bind_text_label(match_digit);
        self.emit_runtime_string_ascii_digits_match(input, true);
        self.asm.jmp_label(done);

        self.asm.bind_text_label(fallback);
        self.emit_native_string_equality(input, pattern);

        self.asm.bind_text_label(done);
    }

    fn emit_runtime_string_ascii_digits_match(
        &mut self,
        input: NativeStringRef,
        exactly_one: bool,
    ) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        let loop_label = self.asm.create_text_label();
        let success = self.asm.create_text_label();
        let failure = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        if exactly_one {
            self.asm.cmp_reg_imm8(Reg::Rdx, 1);
            self.asm.jcc_label(Condition::NotEqual, failure);
        } else {
            self.asm.cmp_reg_imm8(Reg::Rdx, 0);
            self.asm.jcc_label(Condition::Equal, failure);
        }
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, success);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, b'0' as i8);
        self.asm.jcc_label(Condition::Less, failure);
        self.asm.cmp_reg_imm8(Reg::Rax, b'9' as i8);
        self.asm.jcc_label(Condition::Greater, failure);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(success);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(failure);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_string_reverse(&mut self, input: NativeStringRef) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::Rdx);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.mov_imm64(Reg::R9, 0);

        let outer_loop = self.asm.create_text_label();
        let find_start = self.asm.create_text_label();
        let copy_start = self.asm.create_text_label();
        let copy_loop = self.asm.create_text_label();
        let copy_done = self.asm.create_text_label();
        let done = self.asm.create_text_label();

        self.asm.bind_text_label(outer_loop);
        self.asm.cmp_reg_imm8(Reg::R8, 0);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.mov_reg_reg(Reg::R10, Reg::R8);

        self.asm.bind_text_label(find_start);
        self.asm.dec_reg(Reg::R10);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.mov_imm64(Reg::Rdi, 0xc0);
        self.asm.and_reg_reg(Reg::Rax, Reg::Rdi);
        self.asm.mov_imm64(Reg::Rdi, 0x80);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rdi);
        self.asm.jcc_label(Condition::NotEqual, copy_start);
        self.asm.cmp_reg_imm8(Reg::R10, 0);
        self.asm.jcc_label(Condition::Greater, find_start);

        self.asm.bind_text_label(copy_start);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R10);
        self.asm.bind_text_label(copy_loop);
        self.asm.cmp_reg_reg(Reg::Rdi, Reg::R8);
        self.asm.jcc_label(Condition::GreaterEqual, copy_done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::Rdi);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::Rdi);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(copy_loop);

        self.asm.bind_text_label(copy_done);
        self.asm.mov_reg_reg(Reg::R8, Reg::R10);
        self.asm.jmp_label(outer_loop);
        self.asm.bind_text_label(done);

        NativeValue::RuntimeString { data, len }
    }

    fn emit_append_static_string_to_runtime_buffer_dynamic(
        &mut self,
        output_base: Reg,
        output_offset: Reg,
        label: DataLabel,
        len: usize,
        span: Span,
        overflow_message: &str,
    ) {
        self.asm.mov_data_addr(Reg::R10, label);
        self.asm.mov_imm64(Reg::Rcx, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_imm32(Reg::Rcx, len as i32);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(output_offset, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::R10, Reg::Rcx);
        self.asm
            .mov_byte_indexed_reg8(output_base, output_offset, Reg8::Al);
        self.asm.inc_reg(output_offset);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
    }

    fn emit_append_native_string_to_runtime_buffer_offset_label(
        &mut self,
        output: DataLabel,
        offset: DataLabel,
        input: NativeStringRef,
        span: Span,
        overflow_message: &str,
    ) {
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R9, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
    }

    fn emit_copy_native_string_to_runtime_string_buffer(
        &mut self,
        output: DataLabel,
        len: DataLabel,
        input: NativeStringRef,
        span: Span,
        overflow_message: &str,
    ) {
        let offset = self.asm.data_label_with_i64s(&[0]);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.mov_imm64(Reg::R9, 0);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            output,
            offset,
            input,
            span,
            overflow_message,
        );
        self.emit_store_runtime_string_len_from_offset(len, offset);
    }

    fn emit_i64_rax_to_runtime_string_ref(
        &mut self,
        span: Span,
        overflow_message: &str,
    ) -> NativeStringRef {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);
        self.emit_append_i64_rax_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            overflow_message,
        );
        self.emit_store_runtime_string_len_from_offset(len, offset);
        NativeStringRef {
            data,
            len: NativeStringLen::Runtime(len),
        }
    }

    fn emit_append_i64_rax_to_runtime_buffer_offset_label(
        &mut self,
        output: DataLabel,
        offset: DataLabel,
        span: Span,
        overflow_message: &str,
    ) {
        let scratch = self.asm.data_label_with_bytes(&[0; 32]);
        self.asm.mov_data_addr(Reg::Rsi, scratch);
        self.asm.add_reg_imm32(Reg::Rsi, 32);
        self.asm.mov_imm64(Reg::Rcx, 0);

        let nonzero = self.asm.create_text_label();
        let digits = self.asm.create_text_label();
        let digit_loop = self.asm.create_text_label();
        let write = self.asm.create_text_label();

        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::NotEqual, nonzero);
        self.asm.dec_reg(Reg::Rsi);
        self.asm.mov_byte_ptr_reg_imm8(Reg::Rsi, b'0');
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(write);

        self.asm.bind_text_label(nonzero);
        self.asm.xor_reg_reg(Reg::R8, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::GreaterEqual, digits);
        self.asm.neg_reg(Reg::Rax);
        self.asm.mov_imm64(Reg::R8, 1);

        self.asm.bind_text_label(digits);
        self.asm.mov_imm64(Reg::Rbx, 10);
        self.asm.bind_text_label(digit_loop);
        self.asm.xor_reg_reg(Reg::Rdx, Reg::Rdx);
        self.asm.div_reg(Reg::Rbx);
        self.asm.add_reg8_imm8(Reg8::Dl, b'0');
        self.asm.dec_reg(Reg::Rsi);
        self.asm.mov_byte_ptr_reg8(Reg::Rsi, Reg8::Dl);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.test_reg_reg(Reg::Rax, Reg::Rax);
        self.asm.jcc_label(Condition::NotEqual, digit_loop);
        self.asm.cmp_reg_imm8(Reg::R8, 0);
        self.asm.jcc_label(Condition::Equal, write);
        self.asm.dec_reg(Reg::Rsi);
        self.asm.mov_byte_ptr_reg_imm8(Reg::Rsi, b'-');
        self.asm.inc_reg(Reg::Rcx);

        self.asm.bind_text_label(write);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_imm64(Reg::R8, 0);
        let copy_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(copy_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R9, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(copy_loop);
        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
    }

    fn emit_bool_rax_to_runtime_string_ref(
        &mut self,
        span: Span,
        overflow_message: &str,
    ) -> NativeStringRef {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);
        let offset = self.asm.data_label_with_i64s(&[0]);
        self.emit_append_bool_rax_to_runtime_buffer_offset_label(
            data,
            offset,
            span,
            overflow_message,
        );
        self.emit_store_runtime_string_len_from_offset(len, offset);
        NativeStringRef {
            data,
            len: NativeStringLen::Runtime(len),
        }
    }

    fn emit_append_bool_rax_to_runtime_buffer_offset_label(
        &mut self,
        output: DataLabel,
        offset: DataLabel,
        span: Span,
        overflow_message: &str,
    ) {
        let false_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::Equal, false_label);
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            output,
            offset,
            NativeStringRef {
                data: self.true_text,
                len: NativeStringLen::Immediate(4),
            },
            span,
            overflow_message,
        );
        self.asm.jmp_label(done);
        self.asm.bind_text_label(false_label);
        self.emit_append_native_string_to_runtime_buffer_offset_label(
            output,
            offset,
            NativeStringRef {
                data: self.false_text,
                len: NativeStringLen::Immediate(5),
            },
            span,
            overflow_message,
        );
        self.asm.bind_text_label(done);
    }

    fn emit_store_runtime_string_len_from_offset(&mut self, len: DataLabel, offset: DataLabel) {
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
    }

    fn emit_runtime_buffer_capacity_check(
        &mut self,
        output_offset: Reg,
        capacity: usize,
        span: Span,
        message: &str,
    ) {
        let ok = self.asm.create_text_label();
        self.asm.cmp_reg_imm32(output_offset, capacity as i32);
        self.asm.jcc_label(Condition::Less, ok);
        self.emit_runtime_error(span, message);
        self.asm.bind_text_label(ok);
    }

    fn emit_nul_terminated_path_buffer(
        &mut self,
        input: NativeStringRef,
        span: Span,
        name: &str,
    ) -> DataLabel {
        const RUNTIME_PATH_CAP: usize = 65_536;
        let output = self
            .asm
            .data_label_with_bytes(&vec![0; RUNTIME_PATH_CAP + 1]);
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.emit_runtime_buffer_capacity_check(
            Reg::Rdx,
            RUNTIME_PATH_CAP + 1,
            span,
            &format!("{name} runtime path exceeds 65536 bytes"),
        );

        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::Rdx, Reg8::Al);
        output
    }

    fn emit_copy_input_prefix_to_runtime_buffer(
        &mut self,
        input: NativeStringRef,
        output: DataLabel,
        found_offset: DataLabel,
    ) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.asm.mov_data_addr(Reg::R10, found_offset);
        self.asm.load_ptr_disp32(Reg::Rdx, Reg::R10, 0);
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_imm64(Reg::R8, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_append_input_suffix_after_dynamic_replacement(
        &mut self,
        input: NativeStringRef,
        output: DataLabel,
        found_offset: DataLabel,
        from: NativeStringRef,
        offset: DataLabel,
        span: Span,
        overflow_message: &str,
    ) {
        self.asm.mov_data_addr(Reg::Rsi, input.data);
        self.emit_load_native_string_len(Reg::Rdx, input.len);
        self.asm.mov_data_addr(Reg::Rbx, output);
        self.asm.mov_data_addr(Reg::R10, found_offset);
        self.asm.load_ptr_disp32(Reg::R8, Reg::R10, 0);
        self.emit_load_native_string_len(Reg::Rax, from.len);
        self.asm.add_reg_reg(Reg::R8, Reg::Rax);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.load_ptr_disp32(Reg::R9, Reg::R10, 0);
        let loop_label = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, done);
        self.emit_runtime_buffer_capacity_check(Reg::R9, 65_536, span, overflow_message);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R9, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.inc_reg(Reg::R9);
        self.asm.jmp_label(loop_label);
        self.asm.bind_text_label(done);
        self.asm.mov_data_addr(Reg::R10, offset);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);
    }

    fn emit_jump_if_ascii_whitespace(&mut self, byte_reg: Reg, label: TextLabel) {
        for byte in [b' ', b'\t', b'\n', b'\r', 0x0b, 0x0c] {
            self.asm.cmp_reg_imm8(byte_reg, byte as i8);
            self.asm.jcc_label(Condition::Equal, label);
        }
    }

    fn emit_native_string_match_at_current_offset_with_done(&mut self, done: TextLabel) {
        let loop_label = self.asm.create_text_label();
        let equal_label = self.asm.create_text_label();
        let not_equal_label = self.asm.create_text_label();

        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(loop_label);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, equal_label);
        self.asm.mov_reg_reg(Reg::R10, Reg::R9);
        self.asm.add_reg_reg(Reg::R10, Reg::R8);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R10);
        self.asm.movzx_byte_indexed(Reg::Rbx, Reg::Rdi, Reg::R8);
        self.asm.cmp_reg_reg(Reg::Rax, Reg::Rbx);
        self.asm.jcc_label(Condition::NotEqual, not_equal_label);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(loop_label);

        self.asm.bind_text_label(equal_label);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.jmp_label(done);
        self.asm.bind_text_label(not_equal_label);
        self.asm.mov_imm64(Reg::Rax, 0);
        self.asm.bind_text_label(done);
    }

    fn emit_runtime_string_concat(
        &mut self,
        lhs: NativeStringRef,
        rhs: NativeStringRef,
        span: Span,
    ) -> NativeValue {
        const RUNTIME_STRING_CAP: usize = 65_536;
        let data = self.asm.data_label_with_bytes(&vec![0; RUNTIME_STRING_CAP]);
        let len = self.asm.data_label_with_i64s(&[0]);

        self.asm.mov_data_addr(Reg::Rsi, lhs.data);
        self.emit_load_native_string_len(Reg::Rdx, lhs.len);
        self.asm.mov_data_addr(Reg::Rdi, rhs.data);
        self.emit_load_native_string_len(Reg::Rcx, rhs.len);
        self.asm.mov_reg_reg(Reg::R9, Reg::Rdx);
        self.asm.add_reg_reg(Reg::R9, Reg::Rcx);
        self.asm.mov_imm64(Reg::R10, RUNTIME_STRING_CAP as u64);
        self.asm.cmp_reg_reg(Reg::R9, Reg::R10);
        let copy_lhs = self.asm.create_text_label();
        self.asm.jcc_label(Condition::LessEqual, copy_lhs);
        self.emit_runtime_error(span, "string concatenation result exceeds 65536 bytes");

        self.asm.bind_text_label(copy_lhs);
        self.asm.mov_data_addr(Reg::Rbx, data);
        self.asm.mov_data_addr(Reg::R10, len);
        self.asm.store_ptr_disp32(Reg::R10, 0, Reg::R9);

        let lhs_loop = self.asm.create_text_label();
        let rhs_start = self.asm.create_text_label();
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(lhs_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rdx);
        self.asm.jcc_label(Condition::Equal, rhs_start);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rsi, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R8, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(lhs_loop);

        let rhs_loop = self.asm.create_text_label();
        let done = self.asm.create_text_label();
        self.asm.bind_text_label(rhs_start);
        self.asm.mov_imm64(Reg::R8, 0);
        self.asm.bind_text_label(rhs_loop);
        self.asm.cmp_reg_reg(Reg::R8, Reg::Rcx);
        self.asm.jcc_label(Condition::Equal, done);
        self.asm.movzx_byte_indexed(Reg::Rax, Reg::Rdi, Reg::R8);
        self.asm.mov_reg_reg(Reg::R10, Reg::Rdx);
        self.asm.add_reg_reg(Reg::R10, Reg::R8);
        self.asm.mov_byte_indexed_reg8(Reg::Rbx, Reg::R10, Reg8::Al);
        self.asm.inc_reg(Reg::R8);
        self.asm.jmp_label(rhs_loop);

        self.asm.bind_text_label(done);
        NativeValue::RuntimeString { data, len }
    }

    fn is_print_concat_expr(&self, expr: &Expr) -> bool {
        match expr {
            expr if self.file_input_all_print_call_name(expr).is_some() => true,
            Expr::String { .. } => true,
            Expr::Identifier { name, .. } => matches!(
                self.lookup_var(name).map(|slot| slot.value),
                Some(NativeValue::StaticString { .. } | NativeValue::RuntimeString { .. })
            ),
            Expr::Binary {
                lhs,
                op: BinaryOp::Add,
                rhs,
                ..
            } => self.is_print_concat_expr(lhs) || self.is_print_concat_expr(rhs),
            _ => false,
        }
    }

    fn emit_exit_success(&mut self) {
        self.emit_exit_code(0);
    }

    fn emit_runtime_error_text(&mut self, text: &str) {
        let label = self.asm.data_label_with_bytes(text.as_bytes());
        self.emit_write_data(2, label, text.len());
        self.emit_exit_code(1);
    }

    fn runtime_error_prefix(&self, span: Span) -> String {
        let (line, column) = self.source.line_col(span.start);
        format!("{}:{line}:{column}: ", self.source.name())
    }

    fn emit_runtime_error(&mut self, span: Span, message: &str) {
        let text = format!("{}{}\n", self.runtime_error_prefix(span), message);
        self.emit_runtime_error_text(&text);
    }

    fn emit_runtime_error_if_rax_negative(&mut self, span: Span, message: &str) {
        let ok = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::GreaterEqual, ok);
        self.emit_runtime_error(span, message);
        self.asm.bind_text_label(ok);
    }

    fn emit_runtime_error_if_rax_negative_except_errno(
        &mut self,
        span: Span,
        ignored_errno: i8,
        message: &str,
    ) {
        let ok = self.asm.create_text_label();
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::GreaterEqual, ok);
        self.asm.cmp_reg_imm8(Reg::Rax, ignored_errno);
        self.asm.jcc_label(Condition::Equal, ok);
        self.emit_runtime_error(span, message);
        self.asm.bind_text_label(ok);
    }

    fn emit_todo_failed(&mut self, span: Span) {
        self.emit_runtime_error(span, "not implemented yet");
    }

    fn emit_head_empty(&mut self, span: Span) {
        self.emit_runtime_error(span, "head expects a non-empty list");
    }

    fn emit_assertion_failed(&mut self, span: Span) {
        self.emit_runtime_error(span, "assertion failed");
    }

    fn emit_assert_result_failed_static(
        &mut self,
        span: Span,
        expected: &StaticValue,
        actual: &StaticValue,
    ) {
        let message = format!(
            "assertResult failed: expected {} but got {}",
            self.static_value_display_string(expected),
            self.static_value_display_string(actual)
        );
        self.emit_runtime_error(span, &message);
    }

    fn emit_assert_result_failed_static_native(
        &mut self,
        span: Span,
        expected: NativeValue,
        actual: NativeValue,
    ) {
        let (Some(expected), Some(actual)) = (
            self.static_value_from_native(expected),
            self.static_value_from_native(actual),
        ) else {
            self.emit_runtime_error(span, "assertResult failed");
            return;
        };
        self.emit_assert_result_failed_static(span, &expected, &actual);
    }

    fn emit_assert_result_failed_runtime(&mut self, span: Span, value: NativeValue) {
        let prefix_text = format!(
            "{}assertResult failed: expected ",
            self.runtime_error_prefix(span)
        );
        let prefix = self.asm.data_label_with_bytes(prefix_text.as_bytes());
        let middle = self.asm.data_label_with_bytes(b" but got ");
        self.asm.push_reg(Reg::Rax);
        self.asm.push_reg(Reg::Rcx);
        self.emit_write_data(2, prefix, prefix_text.len());
        self.asm.pop_reg(Reg::Rax);
        self.emit_print_value_fragment(2, value);
        self.emit_write_data(2, middle, " but got ".len());
        self.asm.pop_reg(Reg::Rax);
        self.emit_print_value_fragment(2, value);
        self.emit_write_data(2, self.newline, 1);
        self.emit_exit_code(1);
    }

    fn emit_exit_code(&mut self, code: u64) {
        self.asm.mov_imm64(Reg::Rax, 60);
        self.asm.mov_imm64(Reg::Rdi, code);
        self.asm.syscall();
    }

    fn emit_functions(&mut self) -> Result<(), Diagnostic> {
        let mut emitted = HashSet::new();
        while let Some(name) = self
            .function_order
            .iter()
            .find(|name| self.referenced_functions.contains(*name) && !emitted.contains(*name))
            .cloned()
        {
            let function = self
                .functions
                .get(&name)
                .expect("function order should reference existing functions")
                .clone();
            self.emit_function(&function)?;
            emitted.insert(name);
        }
        for name in self.function_order.clone() {
            if emitted.contains(&name) {
                continue;
            }
            let label = self
                .functions
                .get(&name)
                .expect("function order should reference existing functions")
                .label;
            self.asm.bind_text_label(label);
            self.emit_exit_code(1);
        }
        Ok(())
    }

    fn emit_function(&mut self, function: &NativeFunction) -> Result<(), Diagnostic> {
        let saved_scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
        let saved_static_scopes = std::mem::replace(&mut self.static_scopes, vec![HashMap::new()]);
        let saved_scope_base_offsets = std::mem::replace(&mut self.scope_base_offsets, vec![0]);
        let saved_next_stack_offset = self.next_stack_offset;
        self.next_stack_offset = 0;

        self.asm.bind_text_label(function.label);
        self.asm.push_reg(Reg::Rbp);
        self.asm.mov_reg_reg(Reg::Rbp, Reg::Rsp);

        let arg_regs = argument_registers(function.params.len());
        if function.params.len() <= arg_regs.len() {
            for ((param, value), reg) in function
                .params
                .iter()
                .zip(function.param_values.iter().copied())
                .zip(arg_regs)
            {
                let slot = self.allocate_slot(param.clone(), value);
                self.asm.store_rbp_slot(slot.offset, reg);
            }
        } else {
            let param_count = function.params.len();
            for (index, (param, value)) in function
                .params
                .iter()
                .zip(function.param_values.iter().copied())
                .enumerate()
            {
                let slot = self.allocate_slot(param.clone(), value);
                let offset = 16 + ((param_count - 1 - index) * 8);
                self.asm.load_rbp_arg(Reg::Rax, offset as i32);
                self.asm.store_rbp_slot(slot.offset, Reg::Rax);
            }
        }

        for name in &function.captured_top_level_names {
            if self.lookup_var(name).is_some() {
                continue;
            }
            if let Some(value) = lookup_static_value_in_scopes(&saved_static_scopes, name) {
                self.bind_static_runtime_value(name.clone(), value);
                continue;
            }
            if let Some(slot) = lookup_var_slot_in_scopes(&saved_scopes, name)
                && slot.offset == 0
                && matches!(
                    slot.value,
                    NativeValue::RuntimeString { .. } | NativeValue::RuntimeLinesList { .. }
                )
            {
                self.bind_constant(name.clone(), slot.value);
                continue;
            }
            return Err(unsupported(
                function.body.span(),
                "native recursive function capturing mutable top-level binding",
            ));
        }

        let value = self.compile_expr(&function.body)?;
        if value != function.return_value
            && !matches!(
                (value, function.return_value),
                (NativeValue::Unit, NativeValue::Unit)
            )
        {
            return Err(unsupported(
                function.body.span(),
                "native function return value with this type",
            ));
        }
        self.asm.leave();
        self.asm.ret();

        self.scopes = saved_scopes;
        self.static_scopes = saved_static_scopes;
        self.scope_base_offsets = saved_scope_base_offsets;
        self.next_stack_offset = saved_next_stack_offset;
        Ok(())
    }

    fn emit_print_i64_runtime(&mut self) {
        self.asm.bind_text_label(self.print_i64);
        self.asm.push_reg(Reg::Rbp);
        self.asm.mov_reg_reg(Reg::Rbp, Reg::Rsp);
        self.asm.sub_reg_imm8(Reg::Rsp, 48);
        self.asm.mov_reg_reg(Reg::R10, Reg::Rsi);
        self.asm.mov_reg_reg(Reg::Rax, Reg::Rdi);
        let after_newline = self.asm.create_text_label();
        self.asm.lea_reg_rbp_disp8(Reg::Rsi, 0);
        self.asm.mov_imm64(Reg::Rcx, 0);
        self.asm.cmp_reg_imm8(Reg::Rdx, 0);
        self.asm.jcc_label(Condition::Equal, after_newline);
        self.asm.lea_reg_rbp_disp8(Reg::Rsi, -1);
        self.asm.mov_byte_ptr_reg_imm8(Reg::Rsi, b'\n');
        self.asm.mov_imm64(Reg::Rcx, 1);
        self.asm.bind_text_label(after_newline);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        let nonzero = self.asm.create_text_label();
        let digits = self.asm.create_text_label();
        let digit_loop = self.asm.create_text_label();
        let write = self.asm.create_text_label();
        self.asm.jcc_label(Condition::NotEqual, nonzero);
        self.asm.dec_reg(Reg::Rsi);
        self.asm.mov_byte_ptr_reg_imm8(Reg::Rsi, b'0');
        self.asm.inc_reg(Reg::Rcx);
        self.asm.jmp_label(write);
        self.asm.bind_text_label(nonzero);
        self.asm.xor_reg_reg(Reg::R8, Reg::R8);
        self.asm.cmp_reg_imm8(Reg::Rax, 0);
        self.asm.jcc_label(Condition::GreaterEqual, digits);
        self.asm.neg_reg(Reg::Rax);
        self.asm.mov_imm64(Reg::R8, 1);
        self.asm.bind_text_label(digits);
        self.asm.mov_imm64(Reg::Rbx, 10);
        self.asm.bind_text_label(digit_loop);
        self.asm.xor_reg_reg(Reg::Rdx, Reg::Rdx);
        self.asm.div_reg(Reg::Rbx);
        self.asm.add_reg8_imm8(Reg8::Dl, b'0');
        self.asm.dec_reg(Reg::Rsi);
        self.asm.mov_byte_ptr_reg8(Reg::Rsi, Reg8::Dl);
        self.asm.inc_reg(Reg::Rcx);
        self.asm.test_reg_reg(Reg::Rax, Reg::Rax);
        self.asm.jcc_label(Condition::NotEqual, digit_loop);
        self.asm.cmp_reg_imm8(Reg::R8, 0);
        self.asm.jcc_label(Condition::Equal, write);
        self.asm.dec_reg(Reg::Rsi);
        self.asm.mov_byte_ptr_reg_imm8(Reg::Rsi, b'-');
        self.asm.inc_reg(Reg::Rcx);
        self.asm.bind_text_label(write);
        self.asm.mov_imm64(Reg::Rax, 1);
        self.asm.mov_reg_reg(Reg::Rdi, Reg::R10);
        self.asm.mov_reg_reg(Reg::Rdx, Reg::Rcx);
        self.asm.syscall();
        self.asm.leave();
        self.asm.ret();
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.static_scopes.push(HashMap::new());
        self.scope_base_offsets.push(self.next_stack_offset);
    }

    fn pop_scope(&mut self) {
        let base_offset = self
            .scope_base_offsets
            .pop()
            .expect("native compiler scope offsets should be balanced");
        let allocated = self.next_stack_offset - base_offset;
        if allocated > 0 {
            self.asm.add_reg_imm32(Reg::Rsp, allocated);
            self.next_stack_offset = base_offset;
        }
        self.scopes.pop();
        self.static_scopes.pop();
    }

    fn pop_scope_preserving_allocations(&mut self) {
        self.scope_base_offsets
            .pop()
            .expect("native compiler scope offsets should be balanced");
        self.scopes.pop();
        self.static_scopes.pop();
    }

    fn push_temp_reg(&mut self, reg: Reg) {
        self.asm.push_reg(reg);
        self.next_stack_offset += 8;
    }

    fn pop_temp_reg(&mut self, reg: Reg) {
        self.asm.pop_reg(reg);
        self.release_temp_stack(8);
    }

    fn release_temp_stack(&mut self, bytes: usize) {
        let bytes = bytes as i32;
        debug_assert!(
            self.next_stack_offset >= bytes,
            "native compiler temporary stack tracking underflow"
        );
        self.next_stack_offset -= bytes;
    }

    fn allocate_slot(&mut self, name: String, value: NativeValue) -> VarSlot {
        self.next_stack_offset += 8;
        self.asm.sub_reg_imm8(Reg::Rsp, 8);
        let slot = VarSlot {
            offset: self.next_stack_offset,
            value,
        };
        self.scopes
            .last_mut()
            .expect("native compiler always has a scope")
            .insert(name, slot);
        slot
    }

    fn bind_constant(&mut self, name: String, value: NativeValue) -> VarSlot {
        let slot = VarSlot { offset: 0, value };
        if let Some(value) = self.static_value_from_native(value) {
            self.bind_static_value(name.clone(), value);
        }
        self.scopes
            .last_mut()
            .expect("native compiler always has a scope")
            .insert(name, slot);
        slot
    }

    fn bind_existing_slot(&mut self, name: String, slot: VarSlot) {
        self.scopes
            .last_mut()
            .expect("native compiler always has a scope")
            .insert(name, slot);
    }

    fn bind_static_runtime_value(&mut self, name: String, value: StaticValue) {
        let native = self.emit_static_value(&value);
        match native {
            NativeValue::Int | NativeValue::Bool => {
                let slot = self.allocate_slot(name.clone(), native);
                self.asm.store_rbp_slot(slot.offset, Reg::Rax);
                self.bind_static_value(name, value);
            }
            NativeValue::Null
            | NativeValue::Unit
            | NativeValue::StaticFloat { .. }
            | NativeValue::StaticDouble { .. }
            | NativeValue::StaticString { .. }
            | NativeValue::RuntimeString { .. }
            | NativeValue::RuntimeLinesList { .. }
            | NativeValue::StaticIntList { .. }
            | NativeValue::StaticList { .. }
            | NativeValue::StaticRecord { .. }
            | NativeValue::StaticMap { .. }
            | NativeValue::StaticSet { .. }
            | NativeValue::StaticLambda { .. }
            | NativeValue::BuiltinFunction { .. } => {
                self.bind_constant(name, native);
            }
        }
    }

    fn bind_static_value(&mut self, name: String, value: StaticValue) {
        self.static_scopes
            .last_mut()
            .expect("native compiler always has a static scope")
            .insert(name, value);
    }

    fn assign_static_value(&mut self, name: &str, value: StaticValue) {
        if let Some(scope) = self
            .static_scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.contains_key(name))
        {
            scope.insert(name.to_string(), value);
        }
    }

    fn assign_var_value(&mut self, name: &str, value: NativeValue) {
        if let Some(scope) = self
            .scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.contains_key(name))
            && let Some(slot) = scope.get_mut(name)
        {
            slot.value = value;
        }
    }

    fn remove_static_value(&mut self, name: &str) {
        if let Some(scope) = self
            .static_scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.contains_key(name))
        {
            scope.remove(name);
        }
    }

    fn lookup_static_value(&self, name: &str) -> Option<StaticValue> {
        self.static_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn lookup_var(&self, name: &str) -> Option<VarSlot> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VarSlot {
    offset: i32,
    value: NativeValue,
}

#[derive(Clone, Debug)]
struct NativeFunction {
    label: TextLabel,
    params: Vec<String>,
    param_values: Vec<NativeValue>,
    flexible_params: Vec<bool>,
    return_value: NativeValue,
    body: Expr,
    inline_at_call_site: bool,
    flexible_return: bool,
    contains_thread_call: bool,
    captured_top_level_names: HashSet<String>,
}

#[derive(Clone, Debug)]
struct NativeInstanceMethod {
    name: String,
    params: Vec<String>,
    param_annotations: Vec<Option<String>>,
    body: Expr,
}

fn argument_registers(count: usize) -> Vec<Reg> {
    const REGS: [Reg; 6] = [Reg::Rdi, Reg::Rsi, Reg::Rdx, Reg::Rcx, Reg::R8, Reg::R9];
    REGS[..count.min(REGS.len())].to_vec()
}

fn mkdir_prefixes(path: &str) -> Vec<String> {
    let mut prefixes = Vec::new();
    let mut current = std::path::PathBuf::new();
    for component in std::path::Path::new(path).components() {
        match component {
            std::path::Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            std::path::Component::RootDir => current.push(std::path::MAIN_SEPARATOR.to_string()),
            std::path::Component::CurDir => {
                if current.as_os_str().is_empty() {
                    current.push(".");
                }
            }
            std::path::Component::ParentDir => current.push(".."),
            std::path::Component::Normal(name) => {
                current.push(name);
                prefixes.push(current.display().to_string());
            }
        }
    }
    if prefixes.is_empty() {
        prefixes.push(path.to_string());
    }
    prefixes
}

fn virtual_dir_entry(base: &str, entry: &str, full: bool) -> Option<String> {
    let base_path = std::path::Path::new(base);
    let entry_path = std::path::Path::new(entry);
    let parent_matches = if base_path == std::path::Path::new(".") {
        entry_path
            .parent()
            .is_none_or(|parent| parent.as_os_str().is_empty() || parent == base_path)
    } else {
        entry_path.parent() == Some(base_path)
    };
    if !parent_matches {
        return None;
    }
    if full {
        Some(entry.to_string())
    } else {
        entry_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

fn flatten_curried_call<'a>(
    callee: &'a Expr,
    arguments: &'a [Expr],
) -> Option<(&'a str, Vec<&'a [Expr]>)> {
    let mut groups = vec![arguments];
    let mut current = callee;
    while let Expr::Call {
        callee, arguments, ..
    } = current
    {
        groups.push(arguments);
        current = callee;
    }
    let Expr::Identifier { name, .. } = current else {
        return None;
    };
    groups.reverse();
    Some((name.as_str(), groups))
}

fn native_value_from_annotation(annotation: &TypeAnnotation) -> Option<NativeValue> {
    match annotation.text.trim() {
        "Byte" | "Short" | "Int" | "Long" => Some(NativeValue::Int),
        "Boolean" | "Bool" => Some(NativeValue::Bool),
        "Unit" => Some(NativeValue::Unit),
        _ => None,
    }
}

fn native_value_can_be_static_mutable(value: NativeValue) -> bool {
    matches!(
        value,
        NativeValue::Null
            | NativeValue::Unit
            | NativeValue::StaticFloat { .. }
            | NativeValue::StaticDouble { .. }
            | NativeValue::StaticString { .. }
            | NativeValue::StaticIntList { .. }
            | NativeValue::StaticList { .. }
            | NativeValue::StaticRecord { .. }
            | NativeValue::StaticMap { .. }
            | NativeValue::StaticSet { .. }
            | NativeValue::StaticLambda { .. }
            | NativeValue::BuiltinFunction { .. }
    )
}

fn assigned_names_in_expr(expr: &Expr) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_assigned_names(expr, &mut names);
    names
}

fn expr_references_any_name(expr: &Expr, names: &HashSet<String>) -> bool {
    if names.is_empty() {
        return false;
    }
    let shadowed = HashSet::new();
    expr_references_any_name_with_shadowing(expr, names, &shadowed)
}

fn expr_references_any_name_with_shadowing(
    expr: &Expr,
    names: &HashSet<String>,
    shadowed: &HashSet<String>,
) -> bool {
    match expr {
        Expr::Identifier { name, .. } => names.contains(name) && !shadowed.contains(name),
        Expr::VarDecl { name, value, .. } => {
            if expr_references_any_name_with_shadowing(value, names, shadowed) {
                return true;
            }
            names.contains(name) && !shadowed.contains(name)
        }
        Expr::DefDecl { params, body, .. } | Expr::Lambda { params, body, .. } => {
            let mut nested_shadowed = shadowed.clone();
            for param in params {
                nested_shadowed.insert(param.clone());
            }
            expr_references_any_name_with_shadowing(body, names, &nested_shadowed)
        }
        Expr::Assign { name, value, .. } => {
            (names.contains(name) && !shadowed.contains(name))
                || expr_references_any_name_with_shadowing(value, names, shadowed)
        }
        Expr::Cleanup { body, cleanup, .. } => {
            expr_references_any_name_with_shadowing(body, names, shadowed)
                || expr_references_any_name_with_shadowing(cleanup, names, shadowed)
        }
        Expr::While {
            condition, body, ..
        } => {
            expr_references_any_name_with_shadowing(condition, names, shadowed)
                || expr_references_any_name_with_shadowing(body, names, shadowed)
        }
        Expr::Foreach {
            binding,
            iterable,
            body,
            ..
        } => {
            if expr_references_any_name_with_shadowing(iterable, names, shadowed) {
                return true;
            }
            let mut nested_shadowed = shadowed.clone();
            nested_shadowed.insert(binding.clone());
            expr_references_any_name_with_shadowing(body, names, &nested_shadowed)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_references_any_name_with_shadowing(condition, names, shadowed)
                || expr_references_any_name_with_shadowing(then_branch, names, shadowed)
                || else_branch.as_deref().is_some_and(|branch| {
                    expr_references_any_name_with_shadowing(branch, names, shadowed)
                })
        }
        Expr::Unary { expr, .. } | Expr::FieldAccess { target: expr, .. } => {
            expr_references_any_name_with_shadowing(expr, names, shadowed)
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_references_any_name_with_shadowing(lhs, names, shadowed)
                || expr_references_any_name_with_shadowing(rhs, names, shadowed)
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            expr_references_any_name_with_shadowing(callee, names, shadowed)
                || arguments.iter().any(|argument| {
                    expr_references_any_name_with_shadowing(argument, names, shadowed)
                })
        }
        Expr::Block { expressions, .. } => expressions
            .iter()
            .any(|expression| expr_references_any_name_with_shadowing(expression, names, shadowed)),
        Expr::RecordConstructor { arguments, .. }
        | Expr::ListLiteral {
            elements: arguments,
            ..
        } => arguments
            .iter()
            .any(|argument| expr_references_any_name_with_shadowing(argument, names, shadowed)),
        Expr::RecordLiteral { fields, .. } => fields
            .iter()
            .any(|(_, value)| expr_references_any_name_with_shadowing(value, names, shadowed)),
        Expr::MapLiteral { entries, .. } => entries.iter().any(|(key, value)| {
            expr_references_any_name_with_shadowing(key, names, shadowed)
                || expr_references_any_name_with_shadowing(value, names, shadowed)
        }),
        Expr::SetLiteral { elements, .. } => elements
            .iter()
            .any(|element| expr_references_any_name_with_shadowing(element, names, shadowed)),
        Expr::TheoremDeclaration {
            proposition, body, ..
        } => {
            expr_references_any_name_with_shadowing(proposition, names, shadowed)
                || expr_references_any_name_with_shadowing(body, names, shadowed)
        }
        Expr::AxiomDeclaration { proposition, .. } => {
            expr_references_any_name_with_shadowing(proposition, names, shadowed)
        }
        Expr::InstanceDeclaration { methods, .. } => methods
            .iter()
            .any(|method| expr_references_any_name_with_shadowing(method, names, shadowed)),
        Expr::Int { .. }
        | Expr::Double { .. }
        | Expr::Bool { .. }
        | Expr::String { .. }
        | Expr::Null { .. }
        | Expr::Unit { .. }
        | Expr::ModuleHeader { .. }
        | Expr::Import { .. }
        | Expr::RecordDeclaration { .. }
        | Expr::TypeClassDeclaration { .. }
        | Expr::PegRuleBlock { .. } => false,
    }
}

fn collect_assigned_names(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::Assign { name, value, .. } => {
            names.insert(name.clone());
            collect_assigned_names(value, names);
        }
        Expr::VarDecl { value, .. } => collect_assigned_names(value, names),
        Expr::DefDecl { body, .. } | Expr::Lambda { body, .. } => {
            collect_assigned_names(body, names);
        }
        Expr::Cleanup { body, cleanup, .. } => {
            collect_assigned_names(body, names);
            collect_assigned_names(cleanup, names);
        }
        Expr::While {
            condition, body, ..
        } => {
            collect_assigned_names(condition, names);
            collect_assigned_names(body, names);
        }
        Expr::TheoremDeclaration {
            proposition, body, ..
        } => {
            collect_assigned_names(proposition, names);
            collect_assigned_names(body, names);
        }
        Expr::AxiomDeclaration { proposition, .. } => collect_assigned_names(proposition, names),
        Expr::Unary { expr, .. } | Expr::FieldAccess { target: expr, .. } => {
            collect_assigned_names(expr, names);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_assigned_names(lhs, names);
            collect_assigned_names(rhs, names);
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            collect_assigned_names(callee, names);
            for argument in arguments {
                collect_assigned_names(argument, names);
            }
        }
        Expr::RecordConstructor { arguments, .. }
        | Expr::ListLiteral {
            elements: arguments,
            ..
        } => {
            for argument in arguments {
                collect_assigned_names(argument, names);
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_assigned_names(value, names);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (key, value) in entries {
                collect_assigned_names(key, names);
                collect_assigned_names(value, names);
            }
        }
        Expr::SetLiteral { elements, .. } => {
            for element in elements {
                collect_assigned_names(element, names);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_assigned_names(condition, names);
            collect_assigned_names(then_branch, names);
            if let Some(else_branch) = else_branch {
                collect_assigned_names(else_branch, names);
            }
        }
        Expr::Foreach { iterable, body, .. } => {
            collect_assigned_names(iterable, names);
            collect_assigned_names(body, names);
        }
        Expr::Block { expressions, .. }
        | Expr::InstanceDeclaration {
            methods: expressions,
            ..
        } => {
            for expression in expressions {
                collect_assigned_names(expression, names);
            }
        }
        Expr::Int { .. }
        | Expr::Double { .. }
        | Expr::Bool { .. }
        | Expr::String { .. }
        | Expr::Null { .. }
        | Expr::Unit { .. }
        | Expr::Identifier { .. }
        | Expr::ModuleHeader { .. }
        | Expr::Import { .. }
        | Expr::RecordDeclaration { .. }
        | Expr::TypeClassDeclaration { .. }
        | Expr::PegRuleBlock { .. } => {}
    }
}

fn static_expr_is_pure(expr: &Expr) -> bool {
    match expr {
        Expr::Assign { .. } | Expr::While { .. } | Expr::Foreach { .. } | Expr::Cleanup { .. } => {
            false
        }
        Expr::VarDecl { value, mutable, .. } => !*mutable && static_expr_is_pure(value),
        Expr::Unary { expr, .. } => static_expr_is_pure(expr),
        Expr::Binary { lhs, rhs, .. } => static_expr_is_pure(lhs) && static_expr_is_pure(rhs),
        Expr::Call {
            callee, arguments, ..
        } => {
            if let Expr::Identifier { name, .. } = callee.as_ref()
                && matches!(
                    name.as_str(),
                    "println"
                        | "printlnError"
                        | "assert"
                        | "assertResult"
                        | "ToDo"
                        | "sleep"
                        | "thread"
                        | "stopwatch"
                        | "FileOutput#write"
                        | "FileOutput#append"
                        | "FileOutput#writeLines"
                        | "FileOutput#delete"
                        | "stdin"
                        | "stdinLines"
                        | "StandardInput#all"
                        | "StandardInput#lines"
                        | "env"
                        | "Environment#vars"
                        | "getEnv"
                        | "hasEnv"
                        | "Environment#get"
                        | "Environment#exists"
                        | "args"
                        | "CommandLine#args"
                        | "exit"
                        | "Process#exit"
                        | "Dir#current"
                        | "Dir#home"
                        | "Dir#temp"
                        | "Dir#mkdir"
                        | "Dir#mkdirs"
                        | "Dir#delete"
                        | "Dir#copy"
                        | "Dir#move"
                )
            {
                return false;
            }
            static_expr_is_pure(callee) && arguments.iter().all(static_expr_is_pure)
        }
        Expr::FieldAccess { target, .. } => static_expr_is_pure(target),
        Expr::RecordConstructor { arguments, .. }
        | Expr::ListLiteral {
            elements: arguments,
            ..
        } => arguments.iter().all(static_expr_is_pure),
        Expr::RecordLiteral { fields, .. } => {
            fields.iter().all(|(_, value)| static_expr_is_pure(value))
        }
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .all(|(key, value)| static_expr_is_pure(key) && static_expr_is_pure(value)),
        Expr::SetLiteral { elements, .. } => elements.iter().all(static_expr_is_pure),
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            static_expr_is_pure(condition)
                && static_expr_is_pure(then_branch)
                && else_branch.as_deref().is_none_or(static_expr_is_pure)
        }
        Expr::Block { expressions, .. } => expressions.iter().all(static_expr_is_pure),
        Expr::Lambda { .. } => true,
        Expr::DefDecl { body, .. } => static_expr_is_pure(body),
        Expr::InstanceDeclaration { methods, .. } => methods.iter().all(static_expr_is_pure),
        Expr::Int { .. }
        | Expr::Double { .. }
        | Expr::Bool { .. }
        | Expr::String { .. }
        | Expr::Null { .. }
        | Expr::Unit { .. }
        | Expr::Identifier { .. }
        | Expr::ModuleHeader { .. }
        | Expr::Import { .. }
        | Expr::RecordDeclaration { .. }
        | Expr::TypeClassDeclaration { .. }
        | Expr::TheoremDeclaration { .. }
        | Expr::AxiomDeclaration { .. }
        | Expr::PegRuleBlock { .. } => true,
    }
}

fn top_level_thread_aliases(expressions: &[Expr]) -> HashSet<String> {
    let mut aliases = HashSet::from([String::from("thread")]);
    loop {
        let mut changed = false;
        for expression in expressions {
            if let Expr::VarDecl {
                name,
                value,
                mutable: false,
                ..
            } = expression
                && expr_is_thread_alias(value, &aliases)
                && aliases.insert(name.clone())
            {
                changed = true;
            }
        }
        if !changed {
            return aliases;
        }
    }
}

fn expr_is_thread_alias(expr: &Expr, aliases: &HashSet<String>) -> bool {
    matches!(expr, Expr::Identifier { name, .. } if aliases.contains(name))
}

fn top_level_value_names(expressions: &[Expr]) -> HashSet<String> {
    expressions
        .iter()
        .filter_map(|expression| match expression {
            Expr::VarDecl { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn referenced_top_level_names(expr: &Expr, names: &HashSet<String>) -> HashSet<String> {
    names
        .iter()
        .filter(|name| {
            let single = HashSet::from([(*name).clone()]);
            expr_references_any_name(expr, &single)
        })
        .cloned()
        .collect()
}

fn lookup_static_value_in_scopes(
    scopes: &[HashMap<String, StaticValue>],
    name: &str,
) -> Option<StaticValue> {
    scopes
        .iter()
        .rev()
        .find_map(|scope| scope.get(name).cloned())
}

fn lookup_var_slot_in_scopes(scopes: &[HashMap<String, VarSlot>], name: &str) -> Option<VarSlot> {
    scopes
        .iter()
        .rev()
        .find_map(|scope| scope.get(name).copied())
}

fn expr_contains_thread_call(expr: &Expr, thread_aliases: &HashSet<String>) -> bool {
    match expr {
        Expr::Call {
            callee, arguments, ..
        } => {
            if let Expr::Identifier { name, .. } = callee.as_ref()
                && thread_aliases.contains(name)
            {
                return true;
            }
            expr_contains_thread_call(callee, thread_aliases)
                || arguments
                    .iter()
                    .any(|argument| expr_contains_thread_call(argument, thread_aliases))
        }
        Expr::Unary { expr, .. } | Expr::FieldAccess { target: expr, .. } => {
            expr_contains_thread_call(expr, thread_aliases)
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_contains_thread_call(lhs, thread_aliases)
                || expr_contains_thread_call(rhs, thread_aliases)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_contains_thread_call(condition, thread_aliases)
                || expr_contains_thread_call(then_branch, thread_aliases)
                || else_branch
                    .as_deref()
                    .is_some_and(|branch| expr_contains_thread_call(branch, thread_aliases))
        }
        Expr::While {
            condition, body, ..
        } => {
            expr_contains_thread_call(condition, thread_aliases)
                || expr_contains_thread_call(body, thread_aliases)
        }
        Expr::Foreach { iterable, body, .. } => {
            expr_contains_thread_call(iterable, thread_aliases)
                || expr_contains_thread_call(body, thread_aliases)
        }
        Expr::Block { expressions, .. } => expressions
            .iter()
            .any(|expression| expr_contains_thread_call(expression, thread_aliases)),
        Expr::Cleanup { body, cleanup, .. } => {
            expr_contains_thread_call(body, thread_aliases)
                || expr_contains_thread_call(cleanup, thread_aliases)
        }
        Expr::Lambda { body, .. }
        | Expr::DefDecl { body, .. }
        | Expr::TheoremDeclaration { body, .. } => expr_contains_thread_call(body, thread_aliases),
        Expr::InstanceDeclaration { methods, .. } => methods
            .iter()
            .any(|method| expr_contains_thread_call(method, thread_aliases)),
        Expr::RecordConstructor { arguments, .. }
        | Expr::ListLiteral {
            elements: arguments,
            ..
        } => arguments
            .iter()
            .any(|argument| expr_contains_thread_call(argument, thread_aliases)),
        Expr::RecordLiteral { fields, .. } => fields
            .iter()
            .any(|(_, value)| expr_contains_thread_call(value, thread_aliases)),
        Expr::MapLiteral { entries, .. } => entries.iter().any(|(key, value)| {
            expr_contains_thread_call(key, thread_aliases)
                || expr_contains_thread_call(value, thread_aliases)
        }),
        Expr::SetLiteral { elements, .. } => elements
            .iter()
            .any(|element| expr_contains_thread_call(element, thread_aliases)),
        Expr::VarDecl { value, .. } | Expr::Assign { value, .. } => {
            expr_contains_thread_call(value, thread_aliases)
        }
        Expr::AxiomDeclaration { proposition, .. } => {
            expr_contains_thread_call(proposition, thread_aliases)
        }
        Expr::Int { .. }
        | Expr::Double { .. }
        | Expr::Bool { .. }
        | Expr::String { .. }
        | Expr::Null { .. }
        | Expr::Unit { .. }
        | Expr::Identifier { .. }
        | Expr::ModuleHeader { .. }
        | Expr::Import { .. }
        | Expr::RecordDeclaration { .. }
        | Expr::TypeClassDeclaration { .. }
        | Expr::PegRuleBlock { .. } => false,
    }
}

fn expr_shape_equal(lhs: &Expr, rhs: &Expr) -> bool {
    match (lhs, rhs) {
        (
            Expr::Int {
                value: lhs,
                kind: lhs_kind,
                ..
            },
            Expr::Int {
                value: rhs,
                kind: rhs_kind,
                ..
            },
        ) => lhs == rhs && lhs_kind == rhs_kind,
        (
            Expr::Double {
                value: lhs,
                kind: lhs_kind,
                ..
            },
            Expr::Double {
                value: rhs,
                kind: rhs_kind,
                ..
            },
        ) => lhs.to_bits() == rhs.to_bits() && lhs_kind == rhs_kind,
        (Expr::Bool { value: lhs, .. }, Expr::Bool { value: rhs, .. }) => lhs == rhs,
        (Expr::String { value: lhs, .. }, Expr::String { value: rhs, .. }) => lhs == rhs,
        (Expr::Null { .. }, Expr::Null { .. })
        | (Expr::Unit { .. }, Expr::Unit { .. })
        | (Expr::PegRuleBlock { .. }, Expr::PegRuleBlock { .. }) => true,
        (Expr::Identifier { name: lhs, .. }, Expr::Identifier { name: rhs, .. }) => lhs == rhs,
        (Expr::ModuleHeader { name: lhs, .. }, Expr::ModuleHeader { name: rhs, .. }) => lhs == rhs,
        (
            Expr::Import {
                path: lhs_path,
                alias: lhs_alias,
                members: lhs_members,
                excludes: lhs_excludes,
                ..
            },
            Expr::Import {
                path: rhs_path,
                alias: rhs_alias,
                members: rhs_members,
                excludes: rhs_excludes,
                ..
            },
        ) => {
            lhs_path == rhs_path
                && lhs_alias == rhs_alias
                && lhs_members == rhs_members
                && lhs_excludes == rhs_excludes
        }
        (
            Expr::RecordDeclaration {
                name: lhs_name,
                type_params: lhs_type_params,
                fields: lhs_fields,
                ..
            },
            Expr::RecordDeclaration {
                name: rhs_name,
                type_params: rhs_type_params,
                fields: rhs_fields,
                ..
            },
        ) => {
            lhs_name == rhs_name
                && lhs_type_params == rhs_type_params
                && lhs_fields.len() == rhs_fields.len()
                && lhs_fields.iter().zip(rhs_fields.iter()).all(|(lhs, rhs)| {
                    lhs.name == rhs.name
                        && option_annotation_shape_equal(&lhs.annotation, &rhs.annotation)
                })
        }
        (Expr::RecordLiteral { fields: lhs, .. }, Expr::RecordLiteral { fields: rhs, .. }) => {
            lhs.len() == rhs.len()
                && lhs.iter().zip(rhs.iter()).all(
                    |((lhs_name, lhs_value), (rhs_name, rhs_value))| {
                        lhs_name == rhs_name && expr_shape_equal(lhs_value, rhs_value)
                    },
                )
        }
        (
            Expr::TypeClassDeclaration {
                name: lhs_name,
                type_params: lhs_type_params,
                methods: lhs_methods,
                ..
            },
            Expr::TypeClassDeclaration {
                name: rhs_name,
                type_params: rhs_type_params,
                methods: rhs_methods,
                ..
            },
        ) => {
            lhs_name == rhs_name
                && lhs_type_params == rhs_type_params
                && lhs_methods.len() == rhs_methods.len()
                && lhs_methods
                    .iter()
                    .zip(rhs_methods.iter())
                    .all(|(lhs, rhs)| {
                        lhs.name == rhs.name
                            && annotation_shape_equal(&lhs.annotation, &rhs.annotation)
                    })
        }
        (
            Expr::InstanceDeclaration {
                class_name: lhs_class,
                for_type: lhs_for_type,
                for_type_annotation: lhs_annotation,
                constraints: lhs_constraints,
                methods: lhs_methods,
                ..
            },
            Expr::InstanceDeclaration {
                class_name: rhs_class,
                for_type: rhs_for_type,
                for_type_annotation: rhs_annotation,
                constraints: rhs_constraints,
                methods: rhs_methods,
                ..
            },
        ) => {
            lhs_class == rhs_class
                && lhs_for_type == rhs_for_type
                && annotation_shape_equal(lhs_annotation, rhs_annotation)
                && constraints_shape_equal(lhs_constraints, rhs_constraints)
                && expr_slices_shape_equal(lhs_methods, rhs_methods)
        }
        (
            Expr::TheoremDeclaration {
                name: lhs_name,
                params: lhs_params,
                param_annotations: lhs_param_annotations,
                proposition: lhs_proposition,
                body: lhs_body,
                trusted: lhs_trusted,
                ..
            },
            Expr::TheoremDeclaration {
                name: rhs_name,
                params: rhs_params,
                param_annotations: rhs_param_annotations,
                proposition: rhs_proposition,
                body: rhs_body,
                trusted: rhs_trusted,
                ..
            },
        ) => {
            lhs_name == rhs_name
                && lhs_params == rhs_params
                && option_annotations_shape_equal(lhs_param_annotations, rhs_param_annotations)
                && expr_shape_equal(lhs_proposition, rhs_proposition)
                && expr_shape_equal(lhs_body, rhs_body)
                && lhs_trusted == rhs_trusted
        }
        (
            Expr::AxiomDeclaration {
                name: lhs_name,
                params: lhs_params,
                param_annotations: lhs_param_annotations,
                proposition: lhs_proposition,
                ..
            },
            Expr::AxiomDeclaration {
                name: rhs_name,
                params: rhs_params,
                param_annotations: rhs_param_annotations,
                proposition: rhs_proposition,
                ..
            },
        ) => {
            lhs_name == rhs_name
                && lhs_params == rhs_params
                && option_annotations_shape_equal(lhs_param_annotations, rhs_param_annotations)
                && expr_shape_equal(lhs_proposition, rhs_proposition)
        }
        (
            Expr::VarDecl {
                mutable: lhs_mutable,
                name: lhs_name,
                annotation: lhs_annotation,
                value: lhs_value,
                ..
            },
            Expr::VarDecl {
                mutable: rhs_mutable,
                name: rhs_name,
                annotation: rhs_annotation,
                value: rhs_value,
                ..
            },
        ) => {
            lhs_mutable == rhs_mutable
                && lhs_name == rhs_name
                && option_annotation_shape_equal(lhs_annotation, rhs_annotation)
                && expr_shape_equal(lhs_value, rhs_value)
        }
        (
            Expr::DefDecl {
                name: lhs_name,
                type_params: lhs_type_params,
                constraints: lhs_constraints,
                params: lhs_params,
                param_annotations: lhs_param_annotations,
                return_annotation: lhs_return_annotation,
                body: lhs_body,
                ..
            },
            Expr::DefDecl {
                name: rhs_name,
                type_params: rhs_type_params,
                constraints: rhs_constraints,
                params: rhs_params,
                param_annotations: rhs_param_annotations,
                return_annotation: rhs_return_annotation,
                body: rhs_body,
                ..
            },
        ) => {
            lhs_name == rhs_name
                && lhs_type_params == rhs_type_params
                && constraints_shape_equal(lhs_constraints, rhs_constraints)
                && lhs_params == rhs_params
                && option_annotations_shape_equal(lhs_param_annotations, rhs_param_annotations)
                && option_annotation_shape_equal(lhs_return_annotation, rhs_return_annotation)
                && expr_shape_equal(lhs_body, rhs_body)
        }
        (
            Expr::Lambda {
                params: lhs_params,
                param_annotations: lhs_param_annotations,
                body: lhs_body,
                ..
            },
            Expr::Lambda {
                params: rhs_params,
                param_annotations: rhs_param_annotations,
                body: rhs_body,
                ..
            },
        ) => {
            lhs_params == rhs_params
                && option_annotations_shape_equal(lhs_param_annotations, rhs_param_annotations)
                && expr_shape_equal(lhs_body, rhs_body)
        }
        (
            Expr::Assign {
                name: lhs_name,
                value: lhs_value,
                ..
            },
            Expr::Assign {
                name: rhs_name,
                value: rhs_value,
                ..
            },
        ) => lhs_name == rhs_name && expr_shape_equal(lhs_value, rhs_value),
        (
            Expr::Unary {
                op: lhs_op,
                expr: lhs_expr,
                ..
            },
            Expr::Unary {
                op: rhs_op,
                expr: rhs_expr,
                ..
            },
        ) => lhs_op == rhs_op && expr_shape_equal(lhs_expr, rhs_expr),
        (
            Expr::Binary {
                lhs: lhs_lhs,
                op: lhs_op,
                rhs: lhs_rhs,
                ..
            },
            Expr::Binary {
                lhs: rhs_lhs,
                op: rhs_op,
                rhs: rhs_rhs,
                ..
            },
        ) => {
            lhs_op == rhs_op
                && expr_shape_equal(lhs_lhs, rhs_lhs)
                && expr_shape_equal(lhs_rhs, rhs_rhs)
        }
        (
            Expr::Call {
                callee: lhs_callee,
                arguments: lhs_arguments,
                ..
            },
            Expr::Call {
                callee: rhs_callee,
                arguments: rhs_arguments,
                ..
            },
        ) => {
            expr_shape_equal(lhs_callee, rhs_callee)
                && expr_slices_shape_equal(lhs_arguments, rhs_arguments)
        }
        (
            Expr::FieldAccess {
                target: lhs_target,
                field: lhs_field,
                ..
            },
            Expr::FieldAccess {
                target: rhs_target,
                field: rhs_field,
                ..
            },
        ) => lhs_field == rhs_field && expr_shape_equal(lhs_target, rhs_target),
        (
            Expr::Cleanup {
                body: lhs_body,
                cleanup: lhs_cleanup,
                ..
            },
            Expr::Cleanup {
                body: rhs_body,
                cleanup: rhs_cleanup,
                ..
            },
        ) => expr_shape_equal(lhs_body, rhs_body) && expr_shape_equal(lhs_cleanup, rhs_cleanup),
        (
            Expr::RecordConstructor {
                name: lhs_name,
                arguments: lhs_arguments,
                ..
            },
            Expr::RecordConstructor {
                name: rhs_name,
                arguments: rhs_arguments,
                ..
            },
        ) => lhs_name == rhs_name && expr_slices_shape_equal(lhs_arguments, rhs_arguments),
        (Expr::ListLiteral { elements: lhs, .. }, Expr::ListLiteral { elements: rhs, .. })
        | (Expr::SetLiteral { elements: lhs, .. }, Expr::SetLiteral { elements: rhs, .. })
        | (
            Expr::Block {
                expressions: lhs, ..
            },
            Expr::Block {
                expressions: rhs, ..
            },
        ) => expr_slices_shape_equal(lhs, rhs),
        (Expr::MapLiteral { entries: lhs, .. }, Expr::MapLiteral { entries: rhs, .. }) => {
            lhs.len() == rhs.len()
                && lhs
                    .iter()
                    .zip(rhs.iter())
                    .all(|((lhs_key, lhs_value), (rhs_key, rhs_value))| {
                        expr_shape_equal(lhs_key, rhs_key) && expr_shape_equal(lhs_value, rhs_value)
                    })
        }
        (
            Expr::If {
                condition: lhs_condition,
                then_branch: lhs_then,
                else_branch: lhs_else,
                ..
            },
            Expr::If {
                condition: rhs_condition,
                then_branch: rhs_then,
                else_branch: rhs_else,
                ..
            },
        ) => {
            expr_shape_equal(lhs_condition, rhs_condition)
                && expr_shape_equal(lhs_then, rhs_then)
                && option_expr_shape_equal(lhs_else.as_deref(), rhs_else.as_deref())
        }
        (
            Expr::While {
                condition: lhs_condition,
                body: lhs_body,
                ..
            },
            Expr::While {
                condition: rhs_condition,
                body: rhs_body,
                ..
            },
        ) => expr_shape_equal(lhs_condition, rhs_condition) && expr_shape_equal(lhs_body, rhs_body),
        (
            Expr::Foreach {
                binding: lhs_binding,
                iterable: lhs_iterable,
                body: lhs_body,
                ..
            },
            Expr::Foreach {
                binding: rhs_binding,
                iterable: rhs_iterable,
                body: rhs_body,
                ..
            },
        ) => {
            lhs_binding == rhs_binding
                && expr_shape_equal(lhs_iterable, rhs_iterable)
                && expr_shape_equal(lhs_body, rhs_body)
        }
        _ => false,
    }
}

fn expr_slices_shape_equal(lhs: &[Expr], rhs: &[Expr]) -> bool {
    lhs.len() == rhs.len()
        && lhs
            .iter()
            .zip(rhs.iter())
            .all(|(lhs, rhs)| expr_shape_equal(lhs, rhs))
}

fn option_expr_shape_equal(lhs: Option<&Expr>, rhs: Option<&Expr>) -> bool {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => expr_shape_equal(lhs, rhs),
        (None, None) => true,
        _ => false,
    }
}

fn annotation_shape_equal(lhs: &TypeAnnotation, rhs: &TypeAnnotation) -> bool {
    lhs.text == rhs.text
}

fn option_annotation_shape_equal(
    lhs: &Option<TypeAnnotation>,
    rhs: &Option<TypeAnnotation>,
) -> bool {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => annotation_shape_equal(lhs, rhs),
        (None, None) => true,
        _ => false,
    }
}

fn option_annotations_shape_equal(
    lhs: &[Option<TypeAnnotation>],
    rhs: &[Option<TypeAnnotation>],
) -> bool {
    lhs.len() == rhs.len()
        && lhs
            .iter()
            .zip(rhs.iter())
            .all(|(lhs, rhs)| option_annotation_shape_equal(lhs, rhs))
}

fn constraints_shape_equal(lhs: &[TypeClassConstraint], rhs: &[TypeClassConstraint]) -> bool {
    lhs.len() == rhs.len()
        && lhs.iter().zip(rhs.iter()).all(|(lhs, rhs)| {
            lhs.class_name == rhs.class_name
                && lhs.arguments.len() == rhs.arguments.len()
                && lhs
                    .arguments
                    .iter()
                    .zip(rhs.arguments.iter())
                    .all(|(lhs, rhs)| annotation_shape_equal(lhs, rhs))
        })
}

fn native_value_hint_from_expr(expr: &Expr) -> Option<NativeValue> {
    match expr {
        Expr::Int { .. } => Some(NativeValue::Int),
        Expr::Double { value, kind, .. } => match kind {
            FloatLiteralKind::Float => Some(NativeValue::StaticFloat {
                bits: (*value as f32).to_bits(),
            }),
            FloatLiteralKind::Double => Some(NativeValue::StaticDouble {
                bits: value.to_bits(),
            }),
        },
        Expr::Bool { .. } => Some(NativeValue::Bool),
        Expr::Null { .. } => Some(NativeValue::Null),
        Expr::Unit { .. } => Some(NativeValue::Unit),
        Expr::Unary { op, expr, .. } => match op {
            UnaryOp::Plus => native_value_hint_from_expr(expr),
            UnaryOp::Minus => match expr.as_ref() {
                Expr::Double { value, kind, .. } => match kind {
                    FloatLiteralKind::Float => Some(NativeValue::StaticFloat {
                        bits: (-(*value as f32)).to_bits(),
                    }),
                    FloatLiteralKind::Double => Some(NativeValue::StaticDouble {
                        bits: (-*value).to_bits(),
                    }),
                },
                _ => native_value_hint_from_expr(expr),
            },
            UnaryOp::Not => Some(NativeValue::Bool),
        },
        Expr::Binary { op, .. } => match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor => Some(NativeValue::Int),
            BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::LogicalAnd
            | BinaryOp::LogicalOr => Some(NativeValue::Bool),
        },
        Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            let then_value = native_value_hint_from_expr(then_branch)?;
            let else_value = else_branch
                .as_deref()
                .and_then(native_value_hint_from_expr)
                .unwrap_or(NativeValue::Unit);
            (then_value == else_value).then_some(then_value)
        }
        Expr::Block { expressions, .. } => expressions.last().and_then(native_value_hint_from_expr),
        Expr::Cleanup { body, .. } => native_value_hint_from_expr(body),
        Expr::While { .. } | Expr::Foreach { .. } | Expr::PegRuleBlock { .. } => {
            Some(NativeValue::Unit)
        }
        _ => None,
    }
}

fn const_int_expr(expr: &Expr) -> Option<i64> {
    eval_const_int_expr_with_bindings(expr, &[])
}

fn static_value_as_f64(value: &StaticValue) -> Option<f64> {
    match value {
        StaticValue::Int(value) => Some(*value as f64),
        StaticValue::Float(bits) => Some(f32::from_bits(*bits) as f64),
        StaticValue::Double(bits) => Some(f64::from_bits(*bits)),
        _ => None,
    }
}

fn static_non_negative_int_from_value(value: &StaticValue) -> Option<usize> {
    let StaticValue::Int(value) = value else {
        return None;
    };
    (*value >= 0).then_some(*value as usize)
}

fn static_value_as_f32(value: &StaticValue) -> Option<f32> {
    match value {
        StaticValue::Int(value) => Some(*value as f32),
        StaticValue::Float(bits) => Some(f32::from_bits(*bits)),
        _ => None,
    }
}

fn static_numeric_rank(value: &StaticValue) -> Option<u8> {
    match value {
        StaticValue::Int(_) => Some(0),
        StaticValue::Float(_) => Some(1),
        StaticValue::Double(_) => Some(2),
        _ => None,
    }
}

fn static_numeric_binary_value(
    op: BinaryOp,
    lhs: &StaticValue,
    rhs: &StaticValue,
) -> Option<StaticValue> {
    let rank = static_numeric_rank(lhs)?.max(static_numeric_rank(rhs)?);
    match rank {
        0 => {
            let (StaticValue::Int(lhs), StaticValue::Int(rhs)) = (lhs, rhs) else {
                return None;
            };
            match op {
                BinaryOp::Add => lhs.checked_add(*rhs).map(StaticValue::Int),
                BinaryOp::Subtract => lhs.checked_sub(*rhs).map(StaticValue::Int),
                BinaryOp::Multiply => lhs.checked_mul(*rhs).map(StaticValue::Int),
                BinaryOp::Divide if *rhs != 0 => lhs.checked_div(*rhs).map(StaticValue::Int),
                _ => None,
            }
        }
        1 => {
            let lhs = static_value_as_f32(lhs)?;
            let rhs = static_value_as_f32(rhs)?;
            if matches!(op, BinaryOp::Divide) && rhs == 0.0 {
                return None;
            }
            let value = match op {
                BinaryOp::Add => lhs + rhs,
                BinaryOp::Subtract => lhs - rhs,
                BinaryOp::Multiply => lhs * rhs,
                BinaryOp::Divide => lhs / rhs,
                _ => return None,
            };
            Some(StaticValue::Float(value.to_bits()))
        }
        2 => {
            let lhs = static_value_as_f64(lhs)?;
            let rhs = static_value_as_f64(rhs)?;
            if matches!(op, BinaryOp::Divide) && rhs == 0.0 {
                return None;
            }
            let value = match op {
                BinaryOp::Add => lhs + rhs,
                BinaryOp::Subtract => lhs - rhs,
                BinaryOp::Multiply => lhs * rhs,
                BinaryOp::Divide => lhs / rhs,
                _ => return None,
            };
            Some(StaticValue::Double(value.to_bits()))
        }
        _ => None,
    }
}

fn numeric_or_comparison_binary_value(
    op: BinaryOp,
    lhs: &StaticValue,
    rhs: &StaticValue,
) -> Option<StaticValue> {
    match op {
        BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
            static_numeric_binary_value(op, lhs, rhs)
        }
        BinaryOp::Less => Some(StaticValue::Bool(
            static_value_as_f64(lhs)? < static_value_as_f64(rhs)?,
        )),
        BinaryOp::LessEqual => Some(StaticValue::Bool(
            static_value_as_f64(lhs)? <= static_value_as_f64(rhs)?,
        )),
        BinaryOp::Greater => Some(StaticValue::Bool(
            static_value_as_f64(lhs)? > static_value_as_f64(rhs)?,
        )),
        BinaryOp::GreaterEqual => Some(StaticValue::Bool(
            static_value_as_f64(lhs)? >= static_value_as_f64(rhs)?,
        )),
        _ => None,
    }
}

fn format_static_float(bits: u32) -> String {
    let value = f32::from_bits(bits);
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

fn format_static_double(bits: u64) -> String {
    let value = f64::from_bits(bits);
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

fn strip_dynamic_cast(expression: &str) -> String {
    let trimmed = expression.trim();
    if let Some(prefix) = trimmed.strip_suffix(":> *") {
        prefix.trim_end().to_string()
    } else if let Some(prefix) = trimmed.strip_suffix(":>*") {
        prefix.trim_end().to_string()
    } else {
        trimmed.to_string()
    }
}

fn runtime_string_returning_helper(name: &str) -> bool {
    matches!(
        name,
        "toString"
            | "substring"
            | "at"
            | "trim"
            | "trimLeft"
            | "trimRight"
            | "replace"
            | "replaceAll"
            | "toLowerCase"
            | "toUpperCase"
            | "repeat"
            | "reverse"
    )
}

fn simple_regex_is_match(input: &str, pattern: &str) -> bool {
    match pattern {
        ".*" => true,
        "[0-9]+" => !input.is_empty() && input.chars().all(|ch| ch.is_ascii_digit()),
        "[0-9]" => input.chars().count() == 1 && input.chars().all(|ch| ch.is_ascii_digit()),
        _ => input == pattern,
    }
}

fn simple_regex_replace_all(input: &str, pattern: &str, replacement: &str) -> String {
    match pattern {
        "[0-9]" => input
            .chars()
            .map(|ch| {
                if ch.is_ascii_digit() {
                    replacement.to_string()
                } else {
                    ch.to_string()
                }
            })
            .collect(),
        _ => input.replace(pattern, replacement),
    }
}

fn eval_const_int_expr_with_binding(expr: &Expr, name: &str, value: i64) -> Option<i64> {
    eval_const_int_expr_with_bindings(expr, &[(name, value)])
}

fn eval_const_int_expr_with_bindings(expr: &Expr, bindings: &[(&str, i64)]) -> Option<i64> {
    match expr {
        Expr::Int { value, .. } => Some(*value),
        Expr::Identifier {
            name: identifier, ..
        } => bindings
            .iter()
            .find_map(|(name, value)| (*name == identifier).then_some(*value)),
        Expr::Unary {
            op: UnaryOp::Plus,
            expr,
            ..
        } => eval_const_int_expr_with_bindings(expr, bindings),
        Expr::Unary {
            op: UnaryOp::Minus,
            expr,
            ..
        } => eval_const_int_expr_with_bindings(expr, bindings)?.checked_neg(),
        Expr::Binary { lhs, op, rhs, .. } => {
            let lhs = eval_const_int_expr_with_bindings(lhs, bindings)?;
            let rhs = eval_const_int_expr_with_bindings(rhs, bindings)?;
            match op {
                BinaryOp::Add => lhs.checked_add(rhs),
                BinaryOp::Subtract => lhs.checked_sub(rhs),
                BinaryOp::Multiply => lhs.checked_mul(rhs),
                BinaryOp::Divide if rhs != 0 => lhs.checked_div(rhs),
                BinaryOp::BitAnd => Some(lhs & rhs),
                BinaryOp::BitOr => Some(lhs | rhs),
                BinaryOp::BitXor => Some(lhs ^ rhs),
                _ => None,
            }
        }
        _ => None,
    }
}

fn unsupported(span: Span, feature: &str) -> Diagnostic {
    Diagnostic::compile(
        span,
        format!("{feature} is not supported by the native compiler yet"),
    )
}

fn native_feature_name(expr: &Expr) -> &'static str {
    match expr {
        Expr::Double { .. } => "native floating-point code generation",
        Expr::Null { .. } => "native null values",
        Expr::Identifier { .. } => "native variable lookup",
        Expr::RecordLiteral { .. }
        | Expr::RecordDeclaration { .. }
        | Expr::RecordConstructor { .. }
        | Expr::FieldAccess { .. } => "native records",
        Expr::VarDecl { .. } | Expr::Assign { .. } => "native mutable/local bindings",
        Expr::DefDecl { .. } | Expr::Lambda { .. } => "native functions and closures",
        Expr::ListLiteral { .. } | Expr::MapLiteral { .. } | Expr::SetLiteral { .. } => {
            "native collections"
        }
        Expr::Foreach { .. } => "native foreach",
        Expr::Import { .. } | Expr::ModuleHeader { .. } => "native modules",
        Expr::TypeClassDeclaration { .. } | Expr::InstanceDeclaration { .. } => {
            "native typeclasses"
        }
        Expr::TheoremDeclaration { .. } | Expr::AxiomDeclaration { .. } => "native proof values",
        Expr::PegRuleBlock { .. } => "native PEG rule blocks",
        _ => "native expression",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DataLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ListLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RecordLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MapLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SetLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LambdaLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BuiltinLabel(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelocKind {
    Rel32,
    Abs64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelocTarget {
    Text(TextLabel),
    Data(DataLabel),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reloc {
    pos: usize,
    kind: RelocKind,
    target: RelocTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObjectFile {
    text: Vec<u8>,
    data: Vec<u8>,
    text_labels: Vec<usize>,
    data_labels: Vec<usize>,
    relocs: Vec<Reloc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reg {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reg8 {
    Al = 0,
    Dl = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Condition {
    Equal,
    NotEqual,
    Below,
    Above,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl Condition {
    fn setcc_opcode(self) -> u8 {
        match self {
            Self::Equal => 0x94,
            Self::NotEqual => 0x95,
            Self::Below => 0x92,
            Self::Above => 0x97,
            Self::Less => 0x9c,
            Self::LessEqual => 0x9e,
            Self::Greater => 0x9f,
            Self::GreaterEqual => 0x9d,
        }
    }

    fn jcc_opcode(self) -> u8 {
        match self {
            Self::Equal => 0x84,
            Self::NotEqual => 0x85,
            Self::Below => 0x82,
            Self::Above => 0x87,
            Self::Less => 0x8c,
            Self::LessEqual => 0x8e,
            Self::Greater => 0x8f,
            Self::GreaterEqual => 0x8d,
        }
    }
}

#[derive(Default)]
struct Assembler {
    text: Vec<u8>,
    data: Vec<u8>,
    text_labels: Vec<Option<usize>>,
    data_labels: Vec<usize>,
    relocs: Vec<Reloc>,
}

impl Assembler {
    fn new() -> Self {
        Self::default()
    }

    fn finish(self) -> ObjectFile {
        ObjectFile {
            text: self.text,
            data: self.data,
            text_labels: self
                .text_labels
                .into_iter()
                .map(|offset| offset.expect("all text labels should be bound"))
                .collect(),
            data_labels: self.data_labels,
            relocs: self.relocs,
        }
    }

    fn create_text_label(&mut self) -> TextLabel {
        let id = self.text_labels.len();
        self.text_labels.push(None);
        TextLabel(id)
    }

    fn bind_text_label(&mut self, label: TextLabel) {
        let slot = self
            .text_labels
            .get_mut(label.0)
            .expect("text label should exist");
        assert!(slot.is_none(), "text label should be bound once");
        *slot = Some(self.text.len());
    }

    fn data_label_with_bytes(&mut self, bytes: &[u8]) -> DataLabel {
        let id = self.data_labels.len();
        let offset = self.data.len();
        self.data_labels.push(offset);
        self.data.extend_from_slice(bytes);
        DataLabel(id)
    }

    fn data_label_with_i64s(&mut self, values: &[i64]) -> DataLabel {
        let id = self.data_labels.len();
        let offset = self.data.len();
        self.data_labels.push(offset);
        for value in values {
            self.data.extend_from_slice(&value.to_le_bytes());
        }
        DataLabel(id)
    }

    fn data_bytes_for_label(&self, label: DataLabel, len: usize) -> &[u8] {
        let offset = self.data_labels[label.0];
        &self.data[offset..offset + len]
    }

    fn i64s_for_label(&self, label: DataLabel, len: usize) -> Vec<i64> {
        let offset = self.data_labels[label.0];
        self.data[offset..offset + len * 8]
            .chunks_exact(8)
            .map(|bytes| i64::from_le_bytes(bytes.try_into().expect("i64 chunk should be 8 bytes")))
            .collect()
    }

    fn byte(&mut self, value: u8) {
        self.text.push(value);
    }

    fn bytes(&mut self, values: &[u8]) {
        self.text.extend_from_slice(values);
    }

    fn imm32(&mut self, value: i32) {
        self.text.extend_from_slice(&value.to_le_bytes());
    }

    fn imm64(&mut self, value: u64) {
        self.text.extend_from_slice(&value.to_le_bytes());
    }

    fn rex_w(&mut self, reg: Reg, rm: Reg) {
        let r = ((reg as u8) >> 3) & 1;
        let b = ((rm as u8) >> 3) & 1;
        self.byte(0x48 | (r << 2) | b);
    }

    fn modrm(&mut self, reg: u8, rm: u8) {
        self.byte(0xc0 | ((reg & 7) << 3) | (rm & 7));
    }

    fn mov_imm64(&mut self, reg: Reg, value: u64) {
        let reg_code = reg as u8;
        self.byte(0x48 | ((reg_code >> 3) & 1));
        self.byte(0xb8 + (reg_code & 7));
        self.imm64(value);
    }

    fn mov_data_addr(&mut self, reg: Reg, label: DataLabel) {
        let reg_code = reg as u8;
        self.byte(0x48 | ((reg_code >> 3) & 1));
        self.byte(0xb8 + (reg_code & 7));
        let pos = self.text.len();
        self.imm64(0);
        self.relocs.push(Reloc {
            pos,
            kind: RelocKind::Abs64,
            target: RelocTarget::Data(label),
        });
    }

    fn mov_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex_w(src, dst);
        self.byte(0x89);
        self.modrm(src as u8, dst as u8);
    }

    fn load_rbp_slot(&mut self, dst: Reg, offset: i32) {
        self.rex_w(dst, Reg::Rbp);
        self.byte(0x8b);
        self.byte(0x85 | (((dst as u8) & 7) << 3));
        self.text.extend_from_slice(&(-offset).to_le_bytes());
    }

    fn load_rbp_arg(&mut self, dst: Reg, offset: i32) {
        self.rex_w(dst, Reg::Rbp);
        self.byte(0x8b);
        self.byte(0x85 | (((dst as u8) & 7) << 3));
        self.text.extend_from_slice(&offset.to_le_bytes());
    }

    fn load_ptr_disp32(&mut self, dst: Reg, base: Reg, disp: i32) {
        self.rex_w(dst, base);
        self.byte(0x8b);
        self.byte(0x80 | (((dst as u8) & 7) << 3) | ((base as u8) & 7));
        self.text.extend_from_slice(&disp.to_le_bytes());
    }

    fn movzx_byte_indexed(&mut self, dst: Reg, base: Reg, index: Reg) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let index_code = index as u8;
        let r = (dst_code >> 3) & 1;
        let x = (index_code >> 3) & 1;
        let b = (base_code >> 3) & 1;
        self.byte(0x40 | (r << 2) | (x << 1) | b);
        self.bytes(&[0x0f, 0xb6]);
        self.byte(0x04 | ((dst_code & 7) << 3));
        self.byte(((index_code & 7) << 3) | (base_code & 7));
    }

    fn movzx_byte_disp32(&mut self, dst: Reg, base: Reg, disp: i32) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let r = (dst_code >> 3) & 1;
        let b = (base_code >> 3) & 1;
        self.byte(0x40 | (r << 2) | b);
        self.bytes(&[0x0f, 0xb6]);
        self.byte(0x80 | ((dst_code & 7) << 3) | (base_code & 7));
        self.text.extend_from_slice(&disp.to_le_bytes());
    }

    fn store_ptr_disp32(&mut self, base: Reg, disp: i32, src: Reg) {
        self.rex_w(src, base);
        self.byte(0x89);
        self.byte(0x80 | (((src as u8) & 7) << 3) | ((base as u8) & 7));
        self.text.extend_from_slice(&disp.to_le_bytes());
    }

    fn store_rbp_slot(&mut self, offset: i32, src: Reg) {
        self.rex_w(src, Reg::Rbp);
        self.byte(0x89);
        self.byte(0x85 | (((src as u8) & 7) << 3));
        self.text.extend_from_slice(&(-offset).to_le_bytes());
    }

    fn push_reg(&mut self, reg: Reg) {
        let reg_code = reg as u8;
        if reg_code >= 8 {
            self.byte(0x41);
        }
        self.byte(0x50 + (reg_code & 7));
    }

    fn pop_reg(&mut self, reg: Reg) {
        let reg_code = reg as u8;
        if reg_code >= 8 {
            self.byte(0x41);
        }
        self.byte(0x58 + (reg_code & 7));
    }

    fn add_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex_w(src, dst);
        self.byte(0x01);
        self.modrm(src as u8, dst as u8);
    }

    fn sub_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex_w(src, dst);
        self.byte(0x29);
        self.modrm(src as u8, dst as u8);
    }

    fn and_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex_w(src, dst);
        self.byte(0x21);
        self.modrm(src as u8, dst as u8);
    }

    fn or_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex_w(src, dst);
        self.byte(0x09);
        self.modrm(src as u8, dst as u8);
    }

    fn xor_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex_w(src, dst);
        self.byte(0x31);
        self.modrm(src as u8, dst as u8);
    }

    fn imul_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex_w(dst, src);
        self.bytes(&[0x0f, 0xaf]);
        self.modrm(dst as u8, src as u8);
    }

    fn idiv_reg(&mut self, reg: Reg) {
        self.rex_w(Reg::Rdi, reg);
        self.byte(0xf7);
        self.modrm(7, reg as u8);
    }

    fn div_reg(&mut self, reg: Reg) {
        self.rex_w(Reg::Rsi, reg);
        self.byte(0xf7);
        self.modrm(6, reg as u8);
    }

    fn cqo(&mut self) {
        self.bytes(&[0x48, 0x99]);
    }

    fn neg_reg(&mut self, reg: Reg) {
        self.rex_w(Reg::Rbx, reg);
        self.byte(0xf7);
        self.modrm(3, reg as u8);
    }

    fn cmp_reg_reg(&mut self, lhs: Reg, rhs: Reg) {
        self.rex_w(rhs, lhs);
        self.byte(0x39);
        self.modrm(rhs as u8, lhs as u8);
    }

    fn cmp_reg_imm8(&mut self, reg: Reg, imm: i8) {
        self.rex_w(Reg::Rdi, reg);
        self.byte(0x83);
        self.modrm(7, reg as u8);
        self.byte(imm as u8);
    }

    fn cmp_reg_imm32(&mut self, reg: Reg, imm: i32) {
        self.rex_w(Reg::Rdi, reg);
        self.byte(0x81);
        self.modrm(7, reg as u8);
        self.text.extend_from_slice(&imm.to_le_bytes());
    }

    fn test_reg_reg(&mut self, lhs: Reg, rhs: Reg) {
        self.rex_w(rhs, lhs);
        self.byte(0x85);
        self.modrm(rhs as u8, lhs as u8);
    }

    fn setcc_al(&mut self, condition: Condition) {
        self.bytes(&[0x0f, condition.setcc_opcode(), 0xc0]);
    }

    fn movzx_rax_al(&mut self) {
        self.bytes(&[0x48, 0x0f, 0xb6, 0xc0]);
    }

    fn jcc_label(&mut self, condition: Condition, label: TextLabel) {
        self.bytes(&[0x0f, condition.jcc_opcode()]);
        let pos = self.text.len();
        self.imm32(0);
        self.relocs.push(Reloc {
            pos,
            kind: RelocKind::Rel32,
            target: RelocTarget::Text(label),
        });
    }

    fn jmp_label(&mut self, label: TextLabel) {
        self.byte(0xe9);
        let pos = self.text.len();
        self.imm32(0);
        self.relocs.push(Reloc {
            pos,
            kind: RelocKind::Rel32,
            target: RelocTarget::Text(label),
        });
    }

    fn call_label(&mut self, label: TextLabel) {
        self.byte(0xe8);
        let pos = self.text.len();
        self.imm32(0);
        self.relocs.push(Reloc {
            pos,
            kind: RelocKind::Rel32,
            target: RelocTarget::Text(label),
        });
    }

    fn inc_reg(&mut self, reg: Reg) {
        self.rex_w(Reg::Rax, reg);
        self.byte(0xff);
        self.modrm(0, reg as u8);
    }

    fn dec_reg(&mut self, reg: Reg) {
        self.rex_w(Reg::Rcx, reg);
        self.byte(0xff);
        self.modrm(1, reg as u8);
    }

    fn sub_reg_imm8(&mut self, reg: Reg, imm: i8) {
        self.rex_w(Reg::Rbp, reg);
        self.byte(0x83);
        self.modrm(5, reg as u8);
        self.byte(imm as u8);
    }

    fn add_reg_imm32(&mut self, reg: Reg, imm: i32) {
        self.rex_w(Reg::Rax, reg);
        self.byte(0x81);
        self.modrm(0, reg as u8);
        self.text.extend_from_slice(&imm.to_le_bytes());
    }

    fn lea_reg_rbp_disp8(&mut self, dst: Reg, disp: i8) {
        self.rex_w(dst, Reg::Rbp);
        self.byte(0x8d);
        self.byte(0x45 | (((dst as u8) & 7) << 3));
        self.byte(disp as u8);
    }

    fn mov_byte_ptr_reg_imm8(&mut self, base: Reg, imm: u8) {
        self.byte(if (base as u8) >= 8 { 0x41 } else { 0x40 });
        self.byte(0xc6);
        self.byte((base as u8) & 7);
        self.byte(imm);
    }

    fn mov_byte_ptr_reg8(&mut self, base: Reg, src: Reg8) {
        self.byte(if (base as u8) >= 8 { 0x41 } else { 0x40 });
        self.byte(0x88);
        self.byte(((src as u8) << 3) | ((base as u8) & 7));
    }

    fn mov_byte_indexed_reg8(&mut self, base: Reg, index: Reg, src: Reg8) {
        let base_code = base as u8;
        let index_code = index as u8;
        let x = (index_code >> 3) & 1;
        let b = (base_code >> 3) & 1;
        self.byte(0x40 | (x << 1) | b);
        self.byte(0x88);
        self.byte(0x04 | ((src as u8) << 3));
        self.byte(((index_code & 7) << 3) | (base_code & 7));
    }

    fn add_reg8_imm8(&mut self, reg: Reg8, imm: u8) {
        self.bytes(&[0x80, 0xc0 | (reg as u8), imm]);
    }

    fn syscall(&mut self) {
        self.bytes(&[0x0f, 0x05]);
    }

    fn leave(&mut self) {
        self.byte(0xc9);
    }

    fn ret(&mut self) {
        self.byte(0xc3);
    }
}

mod elf {
    use super::{ObjectFile, RelocKind, RelocTarget};

    const PAGE_SIZE: u64 = 0x1000;
    const ELF_HEADER_SIZE: u64 = 64;
    const PROGRAM_HEADER_SIZE: u64 = 56;
    const PROGRAM_HEADER_COUNT: u16 = 2;
    const TEXT_VADDR: u64 = 0x401000;
    const TEXT_OFFSET: u64 = 0x1000;

    pub(super) fn write_executable(mut object: ObjectFile) -> Vec<u8> {
        let data_offset = align_to(TEXT_OFFSET + object.text.len() as u64, PAGE_SIZE);
        let data_vaddr = TEXT_VADDR + (data_offset - TEXT_OFFSET);
        patch_relocations(&mut object, data_vaddr);

        let mut bytes = Vec::new();
        write_elf_header(&mut bytes);
        write_program_header(
            &mut bytes,
            TEXT_OFFSET,
            TEXT_VADDR,
            object.text.len() as u64,
            0x5,
        );
        write_program_header(
            &mut bytes,
            data_offset,
            data_vaddr,
            object.data.len() as u64,
            0x6,
        );
        bytes.resize(TEXT_OFFSET as usize, 0);
        bytes.extend_from_slice(&object.text);
        bytes.resize(data_offset as usize, 0);
        bytes.extend_from_slice(&object.data);
        bytes
    }

    fn align_to(value: u64, align: u64) -> u64 {
        value.div_ceil(align) * align
    }

    fn patch_relocations(object: &mut ObjectFile, data_vaddr: u64) {
        for reloc in &object.relocs {
            let target = match reloc.target {
                RelocTarget::Text(label) => TEXT_VADDR + object.text_labels[label.0] as u64,
                RelocTarget::Data(label) => data_vaddr + object.data_labels[label.0] as u64,
            };
            match reloc.kind {
                RelocKind::Rel32 => {
                    let next = TEXT_VADDR + reloc.pos as u64 + 4;
                    let value = (target as i64 - next as i64) as i32;
                    object.text[reloc.pos..reloc.pos + 4].copy_from_slice(&value.to_le_bytes());
                }
                RelocKind::Abs64 => {
                    object.text[reloc.pos..reloc.pos + 8].copy_from_slice(&target.to_le_bytes());
                }
            }
        }
    }

    fn write_elf_header(bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(b"\x7fELF");
        bytes.extend_from_slice(&[2, 1, 1, 0]);
        bytes.extend_from_slice(&[0; 8]);
        write_u16(bytes, 2);
        write_u16(bytes, 62);
        write_u32(bytes, 1);
        write_u64(bytes, TEXT_VADDR);
        write_u64(bytes, ELF_HEADER_SIZE);
        write_u64(bytes, 0);
        write_u32(bytes, 0);
        write_u16(bytes, ELF_HEADER_SIZE as u16);
        write_u16(bytes, PROGRAM_HEADER_SIZE as u16);
        write_u16(bytes, PROGRAM_HEADER_COUNT);
        write_u16(bytes, 0);
        write_u16(bytes, 0);
        write_u16(bytes, 0);
    }

    fn write_program_header(bytes: &mut Vec<u8>, offset: u64, vaddr: u64, size: u64, flags: u32) {
        write_u32(bytes, 1);
        write_u32(bytes, flags);
        write_u64(bytes, offset);
        write_u64(bytes, vaddr);
        write_u64(bytes, vaddr);
        write_u64(bytes, size);
        write_u64(bytes, size);
        write_u64(bytes, PAGE_SIZE);
    }

    fn write_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_u64(bytes: &mut Vec<u8>, value: u64) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeCompilerConfig, compile_source_to_elf};

    #[test]
    fn emits_elf_header_for_println_int_program() {
        let bytes =
            compile_source_to_elf("<test>", "println(1 + 2)", NativeCompilerConfig::default())
                .expect("program should compile");
        assert_eq!(&bytes[..4], b"\x7fELF");
        assert_eq!(bytes[4], 2);
        assert_eq!(bytes[5], 1);
    }

    #[test]
    fn rejects_unsupported_native_construct_after_typecheck() {
        let source = r#"
            mutable s = "x"
            mutable i = 0
            while(i < 2) {
              s = s + "y"
              i += 1
            }
            println(s)
        "#;
        let error = compile_source_to_elf("<test>", source, NativeCompilerConfig::default())
            .expect_err("dynamic aggregate updates are not native-compiled yet");
        assert!(
            error
                .diagnostic()
                .message
                .contains("native static aggregate assignment inside dynamic control flow")
        );
    }
}
