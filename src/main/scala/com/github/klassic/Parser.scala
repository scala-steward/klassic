package com.github.klassic

import scala.util.parsing.combinator.RegexParsers
import scala.util.parsing.input.{CharSequenceReader, Reader, Position}
import com.github.klassic.AstNode._
import com.github.klassic.TypeDescription._

/**
 * @author Kota Mizushima
 */
class Parser extends RegexParsers {
  override def skipWhitespace = false
  private def not[T](p: => Parser[T], msg: String): Parser[Unit] = {
    not(p) | failure(msg)
  }
  private def and[T](p: => Parser[T], msg: String): Parser[Unit] = {
    not(not(p)) | failure(msg)
  }
  lazy val % : Parser[Location] = Parser{reader => Success(reader.pos, reader)}.map{position =>
    Location(position.line, position.column)
  }
  lazy val EOF: Parser[String] = not(elem(".", (ch: Char) => ch != CharSequenceReader.EofCh), "EOF Expected") ^^ {_.toString}
  lazy val LINEFEED : Parser[String] = ("\r\n" | "\r" | "\n")
  lazy val SEMICOLON: Parser[String] = ";"
  lazy val ANY: Parser[String] = elem(".", (ch: Char) => ch != CharSequenceReader.EofCh) ^^ {_.toString}

  lazy val SPACING: Parser[String] = (COMMENT | "\r\n" | "\r" | "\n" | " " | "\t" | "\b" | "\f").* ^^ {_.mkString}
  lazy val SPACING_WITHOUT_LF: Parser[String] = (COMMENT | "\t" | " " | "\b" | "\f").* ^^ {_.mkString}
  lazy val TERMINATOR: Parser[String] = (LINEFEED | SEMICOLON | EOF) <~ SPACING
  lazy val SEPARATOR: Parser[String] = (LINEFEED | COMMA | EOF | SPACING_WITHOUT_LF) <~ SPACING

  lazy val BLOCK_COMMENT: Parser[Any] = (
    "/*" ~ (not("*/") ~ (BLOCK_COMMENT | ANY)).* ~ "*/"
  )
  lazy val LINE_COMMENT: Parser[Any] = (
    "//" ~ (not(LINEFEED) ~ ANY).* ~ LINEFEED
  )
  lazy val COMMENT: Parser[Any] = BLOCK_COMMENT | LINE_COMMENT


  def CL[T](parser: Parser[T]): Parser[T] = parser <~ SPACING
  def token(parser: Parser[String]): Parser[String] = parser <~ SPACING_WITHOUT_LF
  def unescape(input: String): String = {
    val builder = new java.lang.StringBuilder
    val length = input.length
    var i = 0
    while(i < length - 1) {
      (input.charAt(i), input.charAt(i + 1)) match {
        case ('\\', 'r') => builder.append('\r'); i += 2
        case ('\\', 'n') => builder.append('\n'); i += 2
        case ('\\', 'b') => builder.append('\b'); i += 2
        case ('\\', 'f') => builder.append('\f'); i += 2
        case ('\\', 't') => builder.append('\t'); i += 2
        case ('\\', '\\') => builder.append('\\'); i += 2
        case (ch, _) => builder.append(ch); i += 1
      }
    }
    if(i == length - 1) {
      builder.append(input.charAt(i))
    }
    new String(builder)
  }

  lazy val LT      : Parser[String] = token("<")
  lazy val GT      : Parser[String] = token(">")
  lazy val LTE     : Parser[String] = token("<=")
  lazy val GTE     : Parser[String] = token(">=")
  lazy val PLUS    : Parser[String] = token("+")
  lazy val MINUS   : Parser[String] = token("-")
  lazy val ASTER   : Parser[String] = token("*")
  lazy val SLASH   : Parser[String] = token("/")
  lazy val LPAREN  : Parser[String] = token("(")
  lazy val RPAREN  : Parser[String] = token(")")
  lazy val LBRACE  : Parser[String] = token("{")
  lazy val RBRACE  : Parser[String] = token("}")
  lazy val LBRACKET: Parser[String] = token("[")
  lazy val RBRACKET: Parser[String] = token("]")
  lazy val IF      : Parser[String] = token("if")
  lazy val ELSE    : Parser[String] = token("else")
  lazy val WHILE   : Parser[String] = token("while")
  lazy val FOREACH : Parser[String] = token("foreach")
  lazy val TRUE    : Parser[String] = token("true")
  lazy val FALSE   : Parser[String] = token("false")
  lazy val IN      : Parser[String] = token("in")
  lazy val COMMA   : Parser[String] = token(",")
  lazy val DOT     : Parser[String] = token(".")
  lazy val CLASS   : Parser[String] = token("class")
  lazy val DEF     : Parser[String] = token("def")
  lazy val VAL     : Parser[String] = token("val")
  lazy val EQ      : Parser[String] = token("=")
  lazy val EQEQ    : Parser[String] = token("==")
  lazy val ARROW   : Parser[String] = token("=>")
  lazy val COLON   : Parser[String] = token(":")
  lazy val NEW     : Parser[String] = token("new")
  lazy val QUES    : Parser[String] = token("?")
  lazy val KEYWORDS: Set[String]     = Set(
    "<", ">", "<=", ">=", "+", "-", "*", "/", "{", "}", "[", "]", ":", "?",
    "if", "else", "while", "foreach", "true", "false", "in", ",", ".",
    "class", "def", "val", "=", "==", "=>", "new"
  )

  def typeAnnotation: Parser[TypeDescription] = COLON ~> (
    token("Byte") ^^ {_ => ByteType}
  | token("Short")  ^^ {_ => ShortType}
  | token("Int")  ^^ {_ => IntType}
  | token("Long")  ^^ {_ => LongType}
  | token("Float") ^^ {_ => FloatType}
  | token("Double")  ^^ {_ => DoubleType}
  | token("Boolean")  ^^ {_ => BooleanType}
  | token("?") ^^ {_ => DynamicType}
  )

  //lines ::= line {TERMINATOR expr} [TERMINATOR]
  def lines: Parser[AstNode] = SPACING ~> repsep(line, TERMINATOR) <~ opt(TERMINATOR) ^^ Block

  //line ::= expression | val_declaration | functionDefinition
  def line: Parser[AstNode] = expression | val_declaration | functionDefinition

  //expression ::= assignment | conditional | if | while
  def expression: Parser[AstNode] = assignment | conditional | ifExpression | whileExpression | foreachExpression

  //if ::= "if" "(" expression ")" expression "else" expression
  def ifExpression: Parser[AstNode] = CL(IF) ~ CL(LPAREN) ~> expression ~ CL(RPAREN) ~ expression ~ CL(ELSE) ~ expression ^^ {
    case condition ~ _ ~ positive ~ _ ~ negative => IfExpression(condition, positive, negative)
  }

  //while ::= "while" "(" expression ")" expression
  def whileExpression: Parser[AstNode] = (CL(WHILE) ~ CL(LPAREN)) ~> expression ~ (CL(RPAREN) ~> expression) ^^ {
    case condition ~  body => WhileExpression(condition, body)
  }

  //foreach ::= "foreach" "(" ident "in" expression ")" expression
  def foreachExpression: Parser[AstNode] = ((CL(FOREACH) ~> CL(LPAREN)) ~> (CL(ident) <~ CL(IN)) ~ expression <~ CL(RPAREN)) ~ expression ^^ {
    case variable ~ collection ~ body => ForeachExpression(variable.name, collection, body)
  }

  //conditional ::= add {"<" add | ">" add | "<=" add | ">=" add}
  def conditional: Parser[AstNode] = chainl1(add,
    CL(EQEQ) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.EQUAL, left, right)} |
    CL(LTE) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.LESS_OR_EQUAL, left, right)} |
    CL(GTE) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.GREATER_EQUAL, left, right)} |
    CL(LT) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.LESS_THAN, left, right)} |
    CL(GT) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.GREATER_THAN, left, right)}
  )


  //add ::= term {"+" term | "-" term}
  def add: Parser[AstNode] = chainl1(term,
    CL(PLUS) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.ADD, left, right)}|
    CL(MINUS) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.SUBTRACT, left, right)})

  //term ::= factor {"*" factor | "/" factor}
  def term : Parser[AstNode] = chainl1(unary,
    CL(ASTER) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.MULTIPLY, left, right)}|
    CL(SLASH) ^^ {op => (left:AstNode, right:AstNode) => BinaryExpression(Operator.DIVIDE, left, right)})

  def unary: Parser[AstNode] = (
    CL(MINUS) ~ unary ^^ { case _ ~ operand => MinusOp(operand) }
  | CL(PLUS) ~ unary ^^ { case _ ~ operand => PlusOp(operand) }
  | invocation
  )

  def invocation: Parser[AstNode] = application ~ ((CL(DOT) ~> ident) ~ opt(CL(LPAREN) ~> repsep(expression, CL(COMMA)) <~ RPAREN)).* ^^ {
    case self ~ Nil =>
      self
    case self ~ npList  =>
      npList.foldLeft(self){case (self, name ~ params) => MethodCall(self, name.name, params.getOrElse(Nil))}
  }

  def application: Parser[AstNode] = primary ~ opt(CL(LPAREN) ~> repsep(CL(expression), CL(COMMA)) <~ (SPACING <~ RPAREN))^^ {
    case fac ~ param => {
      param match {
        case Some(p) =>
          FunctionCall(fac, p)
        case None =>
          fac
      }
    }
  }

  //primary ::= intLiteral | stringLiteral | listLiteral | "(" expression ")" | "{" lines "}"
  def primary: Parser[AstNode] = ident | integerLiteral | stringLiteral | listLiteral | newObject | anonFun | CL(LPAREN) ~>expression<~ RPAREN | CL(LBRACE) ~>lines<~ RBRACE | hereDocument | hereExpression

  //intLiteral ::= ["1"-"9"] {"0"-"9"}
  def integerLiteral : Parser[AstNode] = ("""[1-9][0-9]*|0""".r ~ opt("BY"| "L" | "S") ^^ {
    case value ~ None => IntNode(value.toLong.toInt)
    case value ~ Some("L") => LongNode(value.toLong)
    case value ~ Some("S") => ShortNode(value.toShort)
    case value ~ Some("BY") => ByteNode(value.toByte)
  }) <~ SPACING_WITHOUT_LF

  def floatLiteral: Parser[AstNode]= "([1-9][0-9]*|0)\\.[0-9]*".r ~ opt("F") ^^ {
    case value ~ None => DoubleNode(value.toDouble)
    case value ~ Some("F") => FloatNode(value.toFloat)
  }

  def booleanLiteral: Parser[AstNode] = (TRUE | FALSE) ^^ {
    case "true" => BooleanNode(true)
    case "false" => BooleanNode(false)
  }

  //stringLiteral ::= "\"" ((?!")(\[rntfb"'\\]|[^\\]))* "\""
  def stringLiteral : Parser[AstNode] = ("\""~> ("""((?!("|#\{))(\\[rntfb"'\\]|[^\\]))+""".r ^^ {in => StringNode(unescape(in))} | "#{" ~> expression <~ "}").*  <~ "\"" ^^ { values =>
    values.foldLeft(StringNode(""):AstNode) { (node, content) => BinaryExpression(Operator.ADD, node, content) }
  }) <~ SPACING_WITHOUT_LF

  def listLiteral: Parser[AstNode] = CL(LBRACKET) ~> (repsep(CL(expression), SEPARATOR) <~ opt(SEPARATOR)) <~ RBRACKET ^^ ListLiteral

  def fqcn: Parser[String] = (ident ~ (CL(DOT) ~ ident).*) ^^ { case id ~ ids => ids.foldLeft(id.name){ case (a, d ~ e) => a + d + e.name} }

  def rebuild(a: Reader[Char], newSource: String, newOffset: Int): Reader[Char] = new Reader[Char] {
    def atEnd = a.atEnd
    def first = a.first
    def pos = a.pos
    def rest = rebuild(a.rest, newSource, offset + 1)
    override def source = newSource
    override def offset = newOffset
  }

  def cat(a: Reader[Char], b: Reader[Char]): Reader[Char] = {
    val aSource = a.source + b.source.subSequence(b.offset, b.source.length()).toString
    if(a.atEnd) {
      rebuild(b, aSource, a.offset)
    } else {
      new Reader[Char] {
        private lazy val result = cat(a.rest, b)
        def atEnd = a.atEnd
        def first = a.first
        def pos = a.pos
        def rest = result
        override def source = aSource
        override def offset = a.offset
      }
    }
  }

  lazy val oneLine: Parser[String] = regex(""".*(\r\n|\r|\n|$)""".r)

  lazy val hereDocument: Parser[StringNode] = ("""<<[a-zA-Z_][a-zA-Z0-9_]*""".r >> { t =>
    val tag = t.substring(2)
    Parser{in =>
      val Success(temp, rest) = oneLine(in)

      val line = new CharSequenceReader(temp, 0)
      hereDocumentBody(tag).apply(rest) match {
        case Success(value, next) =>
          val source = cat(line, next)
          Success(StringNode(value), source)
        case Failure(msg, next) => Failure(msg, cat(line, next))
        case Error(msg, next) => Error(msg, cat(line, next))
      }
    }
  }) <~ SPACING_WITHOUT_LF

  def hereDocumentBody(beginTag: String): Parser[String] = oneLine >> {line =>
    if(beginTag == line.trim) "" else hereDocumentBody(beginTag) ^^ {result =>
      line + result
    }
  }

  lazy val hereExpression: Parser[AstNode] = ("""<<\$[a-zA-Z_][a-zA-Z0-9_]*""".r >> { t =>
    val tag = t.substring(3)
    Parser{in =>
      val Success(temp, rest) = oneLine(in)

      val line = new CharSequenceReader(temp, 0)
      hereDocumentBody(tag).apply(rest) match {
        case Success(value, next) =>
          val Success(ast, _) = lines(new CharSequenceReader(value, 0))
          val source = cat(line, next)
          Success(ast, source)
        case Failure(msg, next) => Failure(msg, cat(line, next))
        case Error(msg, next) => Error(msg, cat(line, next))
      }
    }
  }) <~ SPACING_WITHOUT_LF

  def ident :Parser[Identifier] = ("""[A-Za-z_][a-zA-Z0-9]*""".r^? {
    case n if !KEYWORDS(n) => n
  } ^^ Identifier) <~ SPACING_WITHOUT_LF

  def assignment: Parser[Assignment] = (ident <~ CL(EQ)) ~ expression ^^ {
    case v ~ value => Assignment(v.name, value)
  }

  // val_declaration ::= "val" ident "=" expression
  def val_declaration:Parser[ValDeclaration] = (CL(VAL) ~> ident ~ opt(typeAnnotation) <~ CL(EQ)) ~ expression ^^ {
    case valName ~ optionalType ~ value => ValDeclaration(valName.name, value)
  }

  // anonFun ::= "(" [param {"," param}] ")" "=>" expression
  def anonFun:Parser[AstNode] = (opt(CL(LPAREN) ~> repsep(ident, CL(COMMA)) <~ CL(RPAREN)) <~ CL(ARROW)) ~ expression ^^ {
    case Some(params) ~ proc => FunctionLiteral(params.map{_.name}, proc)
    case None ~ proc => FunctionLiteral(List(), proc)
  }

  // newObject ::= "new" fqcn "(" [param {"," param} ")"
  def newObject: Parser[AstNode] = CL(NEW) ~> fqcn ~ (opt(CL(LPAREN) ~> repsep(ident, CL(COMMA)) <~ (RPAREN))) ^^ {
    case className ~ Some(params) => NewObject(className, params)
    case className ~ None => NewObject(className, List())
  }

  // functionDefinition ::= "def" ident  ["(" [param {"," param]] ")"] "=" expression
  def functionDefinition:Parser[FunctionDefinition] = CL(DEF) ~> ident ~ opt(CL(LPAREN) ~>repsep(ident ~ opt(typeAnnotation), CL(COMMA)) <~ CL(RPAREN)) ~ opt(typeAnnotation) ~ CL(EQ) ~ expression ^^ {
    case functionName ~ params ~ _ ~ optionalType ~ body => {
        val ps = params match {
          case Some(xs) => xs
          case None => Nil
        }
        FunctionDefinition(
          functionName.name,
          FunctionLiteral(ps.map{ case pname ~ ptype => pname.name}, body)
        )
    }
  }

  def parse(str:String): ParseResult[AstNode] = parseAll(lines, str)
}
