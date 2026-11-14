# Operadores de pipe

| Operador | Significado |
| --- | --- |
| `value |> transform` | Aplica `transform(value)` |
| `transform <| value` | Aplica `transform(value)` |
| `first >>> second` | Cria uma função que aplica `first` e depois `second` |
| `first ->> second` | Sinônimo de composição na mesma direção |

```lia
func increment(n: Int) -> Int { n + 1 }
func double(n: Int) -> Int { n * 2 }

routine main() {
    let value = 3 |> increment |> double
    let same = double <| increment <| 3
    let pipeline = increment >>> double
    let alternative = increment ->> double
    // Todos produzem 8:
    let a = pipeline(3)
    let b = 3 |> alternative
}
```

## Aplicação e precedência

`|>` associa à esquerda: `x |> f |> g` equivale a
`g(f(x))`. `<|` associa à direita: `g <| f <| x` também
equivale a `g(f(x))`.

A composição associa à esquerda e tem precedência maior que os pipes
de aplicação. Assim, `x |> f >>> g` aplica ao valor a composição
de `f` com `g`. Aritmética, comparações, chamadas, especializações
e acessos têm precedência maior que a composição. Para comparar o
resultado de um pipeline, escreva `(x |> f) > 0`. Use parênteses
para explicitar agrupamentos quando misturar `|>` com `<|`.

A aplicação segue a ordem das chamadas existentes: primeiro avalia o
destino da chamada e depois o argumento. Isso inclui expressões que
produzem funções: `input() |> factory()` avalia `factory()`,
depois `input()` e então chama a função produzida. Cada expressão é
avaliada uma vez por aplicação.

Os pipes fornecem um argumento, sem inserir parâmetros em outra
chamada. `x |> factory(extra)` exige que `factory(extra)` retorne
uma função que receba `x`. Para adaptar funções com vários argumentos,
use uma closure:

```lia
func add(a: Int, b: Int) -> Int { a + b }
func example() -> Int {
    3 |> (|n: Int| -> Int { add(n, 2) })
}
```

## Composição tipada

As duas funções devem ser unárias. Para `f: (A) -> B` e
`g: (B) -> C`, `f >>> g` tem tipo `(A) -> C`.
O tipo intermediário deve coincidir; não há conversões implícitas
entre as etapas. Funções genéricas usadas como valores devem ser
especializadas, como `identity<Int> >>> double`. Separe a
especialização e o operador com espaço para distinguir os tokens
`>` e `>>>`.

Os operandos da composição são avaliados uma vez, da esquerda para a
direita, quando ela é criada. Seus corpos só executam quando a função
composta é chamada. As funções obtidas são guardadas por valor; cópias,
retornos, dicionários e argumentos de `spawn` preservam essa semântica.
Capturas de referências não copiáveis continuam sendo rejeitadas.

O compilador transforma a composição em closures tipadas antes das
análises de fluxo, ciclos, terminação e complexidade. Os artefatos
`.liac` contêm essas closures, usando a execução já existente para
valores de função. Chamadas em pipelines e nas etapas compostas
participam das verificações, inclusive quando os destinos são indiretos.
A aplicação de funções nomeadas usa as mesmas regras de recursão
estrutural e otimização de cauda das chamadas normais.

Uma exceção ou falha de avaliação interrompe a aplicação. Se a primeira
etapa de uma composição falhar, a segunda não executa. Exceções podem
ser tratadas pelo `try/catch` aplicável.

Pipes não são expressões constantes de compilação: inicializadores
`const` continuam restritos à linguagem de dados descrita em
[CONSTANTES.md](CONSTANTES.md).

Veja [programs/25_pipes.lia](../../programs/25_pipes.lia) e
[tests/pipes.rs](../../tests/pipes.rs).
