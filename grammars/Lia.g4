grammar Lia;

// ==========================================
// PARSER RULES
// ==========================================

program
    : importDecl* topLevelDecl* EOF
    ;

importDecl
    : 'import' (ID | STRING) ('.' (ID | '{' importItemList '}'))?
    ;

importItemList
    : ID (',' ID)* ','?
    ;

topLevelDecl
    : DOC_COMMENT* (constDecl | typeDecl | enumDecl | funcDecl | routineDecl | loopDecl)
    ;

constDecl
    : 'const' ID (':' typeSpec)? '=' expr
    ;

typeDecl
    : 'type' ID typeParams? ('{' structField (',' structField)* ','? '}'
                          | '=' enumVariant ('|' enumVariant)*)
    ;

enumDecl
    : 'enum' ID typeParams? '{' enumVariant (',' enumVariant)* ','? '}'
    ;

enumVariant
    : DOC_COMMENT* ID ('(' (typeSpec (',' typeSpec)* ','?)? ')')?
    ;

structField
    : DOC_COMMENT* ID ':' typeSpec
    ;

funcDecl
    : 'func' ID typeParams? '(' paramList? ')' ('->' typeSpec)? block
    ;

routineDecl
    : 'routine' ID typeParams? '(' routineParamList? ')' block
    ;

typeParams
    : '<' ID (',' ID)* '>'
    ;

loopDecl
    : 'loop' ID '(' ')' block
    ;

routineParamList
    : routineParam (',' routineParam)*
    ;

routineParam
    : ID ':' typeSpec
    ;

paramList
    : param (',' param)*
    ;

param
    : ID ':' typeSpec
    ;

typeSpec
    : '(' (typeSpec (',' typeSpec)*)? ')' '->' typeSpec
    | listType
    | refType
    | primitiveType
    | genericType
    | customType
    ;

listType
    : 'List' '[' typeSpec ']'
    ;

refType
    : 'Ref' '[' typeSpec ']'
    ;

primitiveType
    : 'Int'
    | 'String'
    | 'Bool'
    ;

customType
    : (ID '.')? ID
    ;

genericType
    : ID '[' typeSpec (',' typeSpec)* ']'
    ;

block
    : '{' statement* '}'
    ;

statement
    : DOC_COMMENT* constDecl
    | varDeclStmt
    | memoryStmt
    | spawnStmt
    | signalStmt
    | throwStmt
    | tryCatchStmt
    | terminateStmt
    | forStmt
    | matchStmt
    | exprStmt
    ;

varDeclStmt
    : 'let' 'mut'? ID (':' typeSpec)? '=' expr
    ;

memoryStmt
    : 'memory' ID '=' expr
    ;

spawnStmt
    : 'spawn' expr
    ;

signalStmt
    : 'signal' ID '(' expr ')'
    ;

throwStmt
    : 'throw' expr
    ;

tryCatchStmt
    : 'try' block 'catch' ID block
    ;

terminateStmt
    : 'terminate'
    ;

forStmt
    : 'for' ID 'in' expr block
    ;

matchStmt
    : 'match' expr '{' matchCase (',' matchCase)* ','? '}'
    ;

matchCase
    : pattern '=>' (block | expr)
    ;

pattern
    : ID ('.' ID)* ('(' (pattern (',' pattern)*)? ')')?
    ;

exprStmt
    : expr
    ;

expr
    : primary                                              # PrimaryExpr
    | 'message'                                            # MessageExpr
    | 'memory' ID                                          # MemoryExpr
    | 'ref' expr                                           # RefExpr
    | 'tailcall' expr                                      # TailCallExpr
    | 'type' ID                                            # TypeOfExpr
    | expr '(' argList? ')'                                # CallExpr
    | expr '<' typeSpec (',' typeSpec)* '>'                  # SpecializeExpr
    | expr '[' expr ']'                                    # IndexExpr
    | expr '.' ID                                          # MemberAccessExpr
    | expr op=('*' | '/') expr                             # MulDivExpr
    | expr op=('+' | '-') expr                             # AddSubExpr
    | expr op=('==' | '!=' | '<' | '>' | '<=' | '>=') expr # RelationalExpr
    ;

argList
    : expr (',' expr)*
    ;

closureExpr
    : '|' paramList? '|' '->' typeSpec block
    ;

primary
    : closureExpr
    | ID
    | INT
    | STRING
    | '(' expr ')'
    | listLiteral
    | dictLiteral
    ;

listLiteral
    : '[' (expr (',' expr)* ','?)? ']'
    ;

dictLiteral
    : '{' ((STRING | ID) ':' expr (',' (STRING | ID) ':' expr)* ','?)? '}'
    ;

// ==========================================
// LEXER RULES
// ==========================================

TYPE_KW    : 'type' ;
ENUM_KW    : 'enum' ;
FUNC_KW    : 'func' ;
ROUTINE_KW : 'routine' ;
LET_KW     : 'let' ;
CONST_KW   : 'const' ;
MUT_KW     : 'mut' ;
IMPORT_KW  : 'import' ;
SPAWN_KW   : 'spawn' ;
SIGNAL_KW  : 'signal' ;
TRY_KW     : 'try' ;
CATCH_KW   : 'catch' ;
THROW_KW   : 'throw' ;
TAILCALL_KW : 'tailcall' ;
LOOP_KW    : 'loop' ;
TERMINATE_KW : 'terminate' ;
FOR_KW     : 'for' ;
IN_KW      : 'in' ;
MATCH_KW   : 'match' ;
REF_KW     : 'ref' ;
MESSAGE_KW : 'message' ;
LIST_KW    : 'List' ;

ID         : [a-zA-Z_] [a-zA-Z0-9_]* ;
INT        : [0-9]+ ;
STRING     : '"' ( '\\"' | ~['"\r\n'] )* '"' ;

WS         : [ \t\r\n]+ -> skip ;
DOC_COMMENT   : '///' (~[/\r\n] ~[\r\n]*)? ;
LINE_COMMENT  : '//' ~[\r\n]* -> skip ;
BLOCK_COMMENT : '/*' .*? '*/' -> skip ;
