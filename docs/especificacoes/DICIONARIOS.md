# Dicionários e JSON

`Dict[T]` associa chaves `String` a valores de tipo `T`. O tipo é
integrado à linguagem, assim como `List[T]`. Literais, consultas e
operações `dict.*` funcionam sem import. `Option[T]`, `Some` e
`None` também fazem parte do escopo padrão. Somente as operações
de conversão `json.*` exigem `import json`.

```lia

func example() -> Int {
    let original: Dict[Int] = {"count": 3, other: 4}
    let updated = dict.set(original, "count", 8)
    // original continua com count = 3.
    option.unwrap_or(updated["count"], 0)
}
```

As chaves dos literais são textos entre aspas ou identificadores usados como
texto; `{name: value}` equivale a `{"name": value}`. Chaves repetidas
no mesmo literal são erros de sintaxe. Vírgula final é permitida.
Consultas aceitam expressões de tipo `String`, por exemplo `d[key()]`.
A consulta `d["missing"]` retorna `None`; uma chave presente retorna
`Some(valor)`, sem precisar importar o tipo de retorno. Acesso por ponto
não consulta chaves.

Valores homogêneos têm seu tipo inferido: `{a: 1, b: 2}` é
`Dict[Int]`. Valores de tipos JSON diferentes podem ser reunidos em
`Dict[Json]`. `{}` sem contexto tem tipo `Dict[Json]`; use uma
anotação ou `dict.empty<Int>()` para outro tipo. Os tipos de dicionários
são invariantes: para aceitar valores heterogêneos em atualizações,
declare `Dict[Json]` desde a criação.

| Operação | Retorno |
| --- | --- |
| `dict.empty<T>()` | `Dict[T]` vazio |
| `dict.get(d, key)` ou `d[key]` | `Option[T]` |
| `dict.set(d, key, value)` | Novo `Dict[T]`, inserindo ou substituindo a chave |
| `dict.remove(d, key)` | Novo `Dict[T]`; chave ausente mantém o conteúdo |
| `dict.has(d, key)` | `Bool` |
| `dict.len(d)` | `Int` |
| `dict.keys(d)` | `List[String]` |
| `dict.values(d)` | `List[T]` |

Todas as operações preservam o dicionário original. Cópias, argumentos,
retornos e capturas de closures mantêm valores independentes, inclusive
em estruturas aninhadas. `for key in d { ... }` percorre chaves em ordem
lexicográfica; `keys` e `values` usam essa mesma ordem. Para percorrer
valores, use `for value in dict.values(d) { ... }`.

Um `Dict[(Int)->Int]` pode armazenar funções e closures. Consulte por
`Option` e associe `Some(f)` antes de chamar `f`. Esses destinos
participam das análises de ciclos, terminação e complexidade. Dicionários
com referências não copiáveis não podem ser capturados nem enviados por
`spawn`.

## Conversões JSON

`Json` representa objetos, arrays, textos, booleanos, números e
`null`. Primitivos e coleções compatíveis podem ser passados onde se
espera `Json`. Funções, closures, referências, registros e enums não
são dados JSON. A conversão de saída de `Json` exige uma consulta
explícita de tipo.

```lia
import json

func read_name(text: String) -> Option[String] {
    match json.parse_dict(text) {
        Ok(data) => match_name(data),
        Err(reason) => None
    }
}
func match_name(data: Dict[Json]) -> Option[String] {
    match data["name"] {
        Some(value) => json.as_string(value),
        None => None
    }
}
```

| Operação | Retorno |
| --- | --- |
| `json.parse(text: String)` | `Result[Json, String]` |
| `json.parse_dict(text: String)` | `Result[Dict[Json], String]`; exige objeto na raiz |
| `json.stringify(value: Json)` | `Result[String, String]` |
| `json.as_dict(value)` | `Option[Dict[Json]]` |
| `json.as_array(value)` | `Option[List[Json]]` |
| `json.as_string(value)` | `Option[String]` |
| `json.as_int(value)` | `Option[Int]` |
| `json.as_bool(value)` | `Option[Bool]` |
| `json.is_null(value)` | `Bool` |
| `json.null()` | `Json` |

`Option` e `Result` estão no escopo padrão, independentemente de
`import json`.
Falhas de parsing e conversão retornam `Err(mensagem)`; consultas de
tipo incompatível retornam `None`. Um valor JSON `null` presente
em um dicionário produz `Some(null)`, distinto de uma chave ausente.

Números JSON grandes e decimais são preservados no ciclo de parsing e
serialização. `as_int` aceita apenas números inteiros representados como
`Int` de 64 bits; decimais e inteiros maiores continuam sendo `Json`.
Não há nova sintaxe de literal decimal nem operações aritméticas sobre
`Json`. No código Lia, booleanos usam `True` e `False`; dentro do
texto JSON usam `true` e `false`.

A serialização ordena chaves e não preserva espaços do texto original.
No parsing JSON, chaves repetidas ficam com o último valor. O texto de
`std.log(d)` serve para exibição; use `json.stringify` para obter JSON
válido com aspas e escapes.

Em braços de `match`, chaves logo após `=>` delimitam um bloco.
Para retornar um literal de dicionário diretamente, escreva
`Some(x) => ({value: x})`.

Veja [programs/21_dictionary_json.lia](../../programs/21_dictionary_json.lia)
para um exemplo executável com JSON, closures e `spawn`.
