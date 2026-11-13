# Algebraic Data Types: enums

Enums são tipos nominais discriminados. Cada variante pode ter zero ou mais
campos posicionais; os valores são copiados por valor, inclusive em `spawn`.

```lia
enum Result { Ok(Int), Err(String) }
func value(result: Result) -> Int {
    match result {
        Ok(number) => number,
        Err(reason) => 0
    }
}
routine main() {
    let success = Ok(42)
    let failure: Result = Err("erro")
    value(success)
}
```

A variante declara tipos de campo, e a construção fornece expressões:
`Ok(42)`. Quantidade, tipos de argumentos e tipo retornado são verificados.
Também é possível escrever `Result.Ok(42)`. Se duas declarações tiverem
variantes com o mesmo nome, a qualificação pelo enum é obrigatória.

## Parâmetros e recursão

```lia
enum Maybe<T> { Some(T), Nothing }
enum Tree<T> { Leaf(T), Branch(Tree[T], Tree[T]) }
func total(tree: Tree[Int]) -> Int {
    match tree {
        Leaf(n) => n,
        Branch(left, right) => total(left) + total(right)
    }
}
```

Os parâmetros de tipo podem ser inferidos de argumentos ou do tipo esperado.
`let empty: Maybe[Int] = Nothing` usa o tipo esperado; `Nothing<Int>` e
`Nothing<Int>()` especificam a instância explicitamente. Variantes sem campos e
sem parâmetros genéricos podem ser escritas como `On` ou `On()`.

Construtores com campos também podem ser usados como funções de assinatura
concreta: `let build: (Int) -> Maybe[Int] = Some<Int>`.

## Padrões e exaustividade

Todo `match`, inclusive em rotinas e blocos sem retorno, deve cobrir os valores
possíveis. A verificação considera padrões aninhados e combinações de campos:
`Pair(True, _)` e `Pair(False, _)` cobrem os dois casos do primeiro campo;
`Pair(False, True)` sozinho não cobre `Pair(False, False)`.

`_` ignora um valor. Identificadores de binding começam com letra minúscula ou
underscore; nomes com inicial maiúscula são interpretados como construtores.
Bindings repetidos no mesmo padrão, variantes de outro enum e quantidades de
campos incorretas são rejeitados. Os bindings pertencem somente ao seu ramo.
Nomes de variantes devem ter inicial maiúscula e ser únicos dentro do enum.
`True`, `False`, `Nil` e `Cons` continuam reservados aos tipos preexistentes.
Enums vazios não são aceitos.

## Integração

Variantes podem guardar listas, outros enums e valores de função. Os destinos
de funções nesses campos participam da análise conservadora de chamadas,
ciclos e complexidade. A decomposição de dados recursivos pode demonstrar o
decréscimo estrutural exigido nas chamadas diretas recursivas.

Um enum é copiável somente se todos os seus payloads forem copiáveis para a
instância de tipos usada. Um enum que contém `Ref[T]` não pode atravessar
`spawn`, mesmo quando o valor está numa variante sem referências.

Imports preservam os tipos e construtores. Por exemplo, após `import model`,
são aceitos `model.Outcome[Int]`, `model.Outcome.Good(3)` e o padrão
`model.Outcome.Good(value)`. Artefatos `.liac` preservam as declarações e são
revalidados antes da execução.

Não há representação numérica implícita, conversões entre enums distintos,
payloads com nomes de campo ou guards de padrão nesta sintaxe. Enums permitem
definir tipos opcionais e resultados próprios; não introduzem aliases de tipo
nem tipos opcionais implícitos na biblioteca padrão.

Exemplo executável: [17_enums.lia](programs/17_enums.lia).
