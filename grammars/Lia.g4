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
    : typeDecl
    | funcDecl
    | routineDecl
    | loopDecl
    ;

typeDecl
    : 'type' ID '{' structField (',' structField)* ','? '}'
    ;

structField
    : ID ':' typeSpec
    ;

funcDecl
    : 'func' ID '(' paramList? ')' ('->' typeSpec)? block
    ;

routineDecl
    : 'routine' ID '(' routineParamList? ')' block
    ;

loopDecl
    : 'loop' ID '(' ')' block
    ;

routineParamList
    : routineParam (',' routineParam)*
    ;

routineParam
    : ID ':' (listType | primitiveType | genericType | customType)
    ;

param
    : ID ':' typeSpec
    ;

typeSpec
    : listType
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
    : varDeclStmt
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
    : ID ('(' pattern (',' pattern)* ')')?
    ;

exprStmt
    : expr
    ;

expr
    : primary                                              # PrimaryExpr
    | 'receive'                                            # ReceiveExpr
    | 'ref' expr                                           # RefExpr
    | expr '(' argList? ')'                                # CallExpr
    | expr '.' ID                                          # MemberAccessExpr
    | expr op=('*' | '/') expr                             # MulDivExpr
    | expr op=('+' | '-') expr                             # AddSubExpr
    | expr op=('==' | '!=' | '<' | '>' | '<=' | '>=') expr # RelationalExpr
    ;

argList
    : expr (',' expr)*
    ;

primary
    : ID
    | INT
    | STRING
    | '(' expr ')'
    | listLiteral
    ;

listLiteral
    : '[' (expr (',' expr)* ','?)? ']'
    ;

// ==========================================
// LEXER RULES
// ==========================================

TYPE_KW    : 'type' ;
FUNC_KW    : 'func' ;
ROUTINE_KW : 'routine' ;
LET_KW     : 'let' ;
MUT_KW     : 'mut' ;
IMPORT_KW  : 'import' ;
SPAWN_KW   : 'spawn' ;
SIGNAL_KW  : 'signal' ;
TRY_KW     : 'try' ;
CATCH_KW   : 'catch' ;
THROW_KW   : 'throw' ;
LOOP_KW    : 'loop' ;
TERMINATE_KW : 'terminate' ;
FOR_KW     : 'for' ;
IN_KW      : 'in' ;
MATCH_KW   : 'match' ;
REF_KW     : 'ref' ;
RECEIVE_KW : 'receive' ;
LIST_KW    : 'List' ;

ID         : [a-zA-Z_] [a-zA-Z0-9_]* ;
INT        : [0-9]+ ;
STRING     : '"' ( '\\"' | ~['"\r\n'] )* '"' ;

WS         : [ \t\r\n]+ -> skip ;
LINE_COMMENT  : '//' ~[\r\n]* -> skip ;
BLOCK_COMMENT : '/*' .*? '*/' -> skip ;
