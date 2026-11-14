# Doc-strings

Comentários iniciados por exatamente três barras, `///`, documentam a
declaração seguinte. O texto é Markdown e fica armazenado no AST.

```lia
/// Soma dois inteiros.
///
/// - a: primeira parcela.
/// - b: segunda parcela.
///
/// Retorna a soma como Int.
func add(a: Int, b: Int) -> Int { a + b }
```

Linhas consecutivas formam um bloco. Uma linha `///` vazia separa
parágrafos. O lexer remove as três barras e, se presente, um espaço
seguinte; preserva o restante da indentação para listas e exemplos.
Unicode e arquivos com quebras de linha LF ou CRLF são aceitos.

É possível documentar:

- Funções, rotinas e loops persistentes.
- Tipos de registro e enums, inclusive a sintaxe de soma `type ... = ...`.
- Constantes globais e locais.
- Campos de registros e variantes de enums.

```lia
/// Coordenadas de um ponto.
type Point {
    /// Posição horizontal.
    x: Int,
    /// Posição vertical.
    y: Int
}

/// Resultado de uma procura.
enum Lookup<T> {
    /// Valor encontrado.
    Found(T),
    /// Nenhum valor disponível.
    Missing
}
```

Comentários comuns `//`, `////` e `/* ... */` continuam sendo
ignorados. Espaços e comentários comuns entre um bloco de documentação
e sua declaração não interrompem a associação; isso permite usar o
comando `annotate` sem perder doc-strings.

Um bloco `///` sem declaração seguinte é erro de sintaxe. Imports,
parâmetros, variáveis `let` e expressões não recebem doc-strings; nesses
locais, use comentários comuns ou descreva-os no bloco da função.
Imports devem vir antes das declarações documentadas. O formato
implementado é `///`; strings entre aspas continuam sendo valores
da linguagem.

## Gerar documentação

```sh
# Escrever Markdown na saída padrão:
cargo run -- doc programs/23_docstrings.lia

# Gravar em arquivo:
cargo run -- doc programs/23_docstrings.lia -o docs/docstrings-api.md
```

O executável também aceita `lia docs` como sinônimo de `lia doc`.
O comando resolve imports e valida constantes, tipos, chamadas e
terminação antes de produzir a saída. Ele não executa funções, rotinas,
loops nem os exemplos escritos nos comentários.

A documentação inclui todas as declarações do próprio arquivo, na
ordem original, mesmo quando não possuem texto descritivo. O gerador
mostra assinaturas, parâmetros, retornos, campos e variantes com seus
tipos. Declarações importadas são verificadas, mas não são duplicadas
na API do arquivo consumidor; a seção de imports registra a dependência.

O texto Markdown é preservado, incluindo blocos de código. Descrições
de parâmetros podem ser listas, como no exemplo acima; não há uma
sintaxe adicional de tags como `@param`.

O arquivo de saída só é escrito após a validação. Erros não substituem
um arquivo de documentação existente, e o caminho de saída não pode ser
o próprio arquivo-fonte.

Doc-strings das declarações que permanecem no programa são preservadas
nos artefatos `.liac`. Artefatos antigos, sem esse campo, continuam
aceitos. A geração usa o fonte `.lia`, preservando também assinaturas
e comentários de constantes globais, que são removidas pela avaliação
em tempo de compilação. Constantes locais podem ter metadados no AST
do parser, mas não são listadas na API pública gerada.

Veja o [programa de exemplo](../../programs/23_docstrings.lia) e sua
[documentação gerada](../docstrings-api.md).
