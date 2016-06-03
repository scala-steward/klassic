package com.github.klassic

import com.github.klassic.AstNode._
import com.github.klassic.TypeDescription._

import scala.collection.mutable
/**
  * Created by kota_mizushima on 2016/06/02.
  */
class TypeChecker {
  def isAssignableFrom(expectedType: TypeDescription, actualType: TypeDescription): Boolean = {
    if(expectedType == ErrorType || actualType == ErrorType) {
      false
    } else if(expectedType == DynamicType) {
      true
    } else if(actualType == DynamicType) {
      true
    } else {
      expectedType == actualType
    }
  }
  def typed(node: AstNode): TypeDescription = {
    typeCheck(node, TypeEnvironment(mutable.Map.empty, None))
  }
  def typeCheck(node: AstNode, environment : TypeEnvironment): TypeDescription = {
    node match {
      case Block(expressions) =>
        expressions match {
          case Nil => UnitType
          case x::xs =>
            val xType = typeCheck(x, environment)
            xs.foldLeft(xType){(b, e) => typeCheck(e, environment)}
        }
      case IntNode(_) => IntType
      case ShortNode(_) => ShortType
      case ByteNode(_) => ByteType
      case LongNode(_) => LongType
      case FloatNode(_) => FloatType
      case DoubleNode(_) => DoubleType
      case BooleanNode(_) => BooleanType
      case Assignment(variable, value) =>
        val result = environment.lookup(variable) match {
          case None =>
            throw new InterpreterException(s"variable ${value} is not defined")
          case Some(variableType) =>
            val valueType = typeCheck(value, environment)
            if(!isAssignableFrom(variableType, valueType)) {
              throw new InterpreterException(s"expected type: ${variableType}, actual type: ${valueType}")
            }
            UnitType
        }
        result
      case IfExpression(cond: AstNode, pos: AstNode, neg: AstNode) =>
        val condType = typeCheck(cond, environment)
        if(condType != BooleanType) {
          throw InterpreterException(s"condition type must be Boolean, actual: ${condType}")
        } else {
          val posType = typeCheck(pos, environment)
          val negType = typeCheck(neg, environment)
          if(isAssignableFrom(posType, negType)) {
            throw new InterpreterException(s"type ${posType} and type ${negType} is incomparable")
          }
          if(posType == DynamicType)
            DynamicType
          else if(negType == DynamicType)
            DynamicType
          else
            posType
        }
      case ValDeclaration(variable: String, optVariableType: Option[TypeDescription], value: AstNode) =>
        if(environment.variables.contains(variable)) {
          throw new InterruptedException(s"variable ${variable} is already defined")
        }
        val valueType = typeCheck(value, environment)
        optVariableType match {
          case Some(variableType) =>
            if(isAssignableFrom(variableType, valueType)) {
              environment.variables(variable) = variableType
            } else {
              throw new InterpreterException(s"expected type: ${variableType}, but actual type: ${valueType}")
            }
          case None =>
            environment.variables(variable) = valueType
        }
        UnitType
      case ForeachExpression(name: String, collection: AstNode, body: AstNode) => ???
      case WhileExpression(condition: AstNode, body: AstNode) =>
        val conditionType = typeCheck(condition, environment)
        if(conditionType != BooleanType) {
          throw InterpreterException(s"condition type must be Boolean, actual: ${conditionType}")
        } else {
          typeCheck(body, environment)
          UnitType
        }
      case BinaryExpression(Operator.EQUAL, left, right) =>
        val lType = typeCheck(left, environment)
        val rType = typeCheck(right, environment)
        if(isAssignableFrom(lType, rType)) {
          BooleanType
        } else {
          throw InterpreterException(s"expected type: ${lType}, actual type: ${rType}")
        }
      case BinaryExpression(Operator.LESS_THAN, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("comparison operation must be done between the same numeric types")
        }
      case BinaryExpression(Operator.GREATER_THAN, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("comparison operation must be done between the same numeric types")
        }
      case BinaryExpression(Operator.LESS_OR_EQUAL, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("comparison operation must be done between the same numeric types")
        }
      case BinaryExpression(Operator.GREATER_EQUAL, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("comparison operation must be done between the same numeric types")
        }
      case BinaryExpression(Operator.ADD, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("arithmetic operation must be done between the same numeric types")
        }
      case BinaryExpression(Operator.SUBTRACT, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("arithmetic operation must be done between the same numeric types")
        }
      case BinaryExpression(Operator.MULTIPLY, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("arithmetic operation must be done between the same numeric types")
        }
      case BinaryExpression(Operator.DIVIDE, left, right) =>
        (typeCheck(left, environment), typeCheck(right, environment)) match{
          case (IntType, IntType) => IntType
          case (LongType, LongType) => LongType
          case (ShortType, ShortType) => ShortType
          case (ByteType, ByteType) => ByteType
          case (FloatType, FloatType) => FloatType
          case (DoubleType, DoubleType) => DoubleType
          case (lType, DynamicType) => lType
          case (DynamicType, rtype) => rtype
          case _ => throw InterpreterException("arithmetic operation must be done between the same numeric types")
        }
      case MinusOp(operand: AstNode) =>
        typeCheck(operand, environment) match {
          case ByteType => ByteType
          case IntType => IntType
          case ShortType => ShortType
          case LongType => LongType
          case FloatType => FloatType
          case DoubleType => DoubleType
          case DynamicType => DynamicType
          case otherwise => throw InterpreterException(s"expected: Numeric type, actual: ${otherwise}")
        }
      case PlusOp(operand: AstNode) =>
        typeCheck(operand, environment) match {
          case ByteType => ByteType
          case IntType => IntType
          case ShortType => ShortType
          case LongType => LongType
          case FloatType => FloatType
          case DoubleType => DoubleType
          case DynamicType => DynamicType
          case otherwise => throw InterpreterException(s"expected: Numeric type, actual: ${otherwise}")
        }
      case StringNode(value: String) =>
        DynamicType
      case Identifier(name: String) =>
        environment.lookup(name) match {
          case None => throw InterpreterException(s"variable ${name} is not found")
          case Some(description) => description
        }
      case FunctionLiteral(params: List[FormalParameter], proc: AstNode) =>
        val paramsMap = mutable.Map(params.map{p => p.name -> p.description}:_*)
        val newEnvironment = TypeEnvironment(paramsMap, Some(environment))
        val paramTypes = params.map{_.description}
        val returnType = typeCheck(proc, newEnvironment)
        FunctionType(paramTypes, returnType)
      case FunctionDefinition(name: String, func: FunctionLiteral) =>
        if(environment.variables.contains(name)) {
          throw new InterruptedException(s"function ${name} is already defined")
        }
        environment.variables(name) = typeCheck(func, environment)
        UnitType
      case FunctionCall(func: AstNode, params: List[AstNode]) =>
        val funcType: FunctionType = typeCheck(func, environment) match {
          case f@FunctionType(_, _) => f
          case otherwise => throw InterpreterException(s"expected: function type, actual type: ${otherwise}")
        }
        val actualParamTypes = params.map(p => typeCheck(p, environment))
        if(funcType.paramTypes.length != actualParamTypes.length) {
          throw InterpreterException(s"expected length: ${funcType.paramTypes.length}, actual length: ${actualParamTypes.length}")
        }
        funcType.paramTypes.zip(actualParamTypes).foreach { case (expectedType, actualType) =>
            if(!isAssignableFrom(expectedType, actualType)){
              throw InterpreterException(s"expected type: ${expectedType}, actual type:${actualType}")
            }
        }
        funcType.returnType
      case ListLiteral(elements: List[AstNode]) =>
        elements.foreach(e => typeCheck(e, environment))
        DynamicType
      case NewObject(className: String, params: List[AstNode]) =>
        params.foreach(p => typeCheck(p, environment))
        DynamicType
      case MethodCall(receiver: AstNode, name: String, params: List[AstNode]) =>
        val receiverType = typeCheck(receiver, environment)
        if(receiverType != DynamicType) {
          throw InterpreterException(s"expected: [*], actual: ${receiverType}")
        }
        params.foreach(p => typeCheck(p, environment))
        DynamicType
      case otherwise =>
        throw InterpreterPanic(otherwise.toString)
    }
  }
}
