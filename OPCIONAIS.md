# Tipos opcionais e resultados

A ausência e os erros são valores discriminados e tipados. Não existe conversão
implícita de `null`, de um valor comum para um opcional ou de um erro para uma
exceção.

## Option

```lia
import option
func increment(value: Option[Int]) -> Int {
    match value {
        Some(n) => n + 1,
        None => 0
    }
}
routine main() {
    let present: Option[Int] = Some(4)
    let absent: Option[Int] = None
    increment(present)
    option.unwrap_or(absent, 9)
}
```

O módulo embutido `option` declara `Option[T] = Some(T) | None`. O tipo do
payload é inferido dos argumentos ou do contexto. Use uma anotação, como acima,
ou especialização explícita quando faltarem informações: `None<Int>`.
Também são aceitos `Option.Some(4)`, `Option.None<Int>` e os nomes qualificados
`option.Option.Some(4)` / `option.Option.None<Int>`.

Os helpers são funções genéricas comuns:

| Função | Comportamento |
|---|---|
| `option.is_some(value)` | Retorna `True` para `Some`. |
| `option.is_none(value)` | Retorna `True` para `None`. |
| `option.unwrap_or(value, fallback)` | Obtém o payload ou o fallback já avaliado. |
| `option.unwrap_or_else(value, fallback)` | Chama `fallback: () -> T` somente para `None`. |
| `option.map(value, f)` | Transforma o payload de `Some` com `f: (T) -> U`. |
| `option.and_then(value, f)` | Encadeia `f: (T) -> Option[U]`, preservando a ausência. |

`unwrap_or` segue a avaliação estrita normal dos argumentos: o fallback é
avaliado mesmo em `Some`. Para avaliação preguiçosa, passe uma closure a
`unwrap_or_else`.

## Result

```lia
import result
func divide(a: Int, b: Int) -> Result[Int, String] {
    match b == 0 {
        True => Err("division by zero"),
        False => Ok(a / b)
    }
}
```

O módulo `result` declara `Result[T, E] = Ok(T) | Err(E)`. Uma instância fixa
ambos os tipos. Uma anotação ou retorno declarado fornece o contexto para
`Ok(4)` e `Err("erro")`; sem esse contexto, use `Ok<Int, String>(4)` ou
`Err<Int, String>("erro")`.

| Função | Comportamento |
|---|---|
| `result.is_ok(value)` / `result.is_err(value)` | Identifica a variante. |
| `result.unwrap_or(value, fallback)` | Obtém `T` ou um fallback já avaliado. |
| `result.unwrap_or_else(value, fallback)` | Chama `fallback: (E) -> T` somente para `Err`. |
| `result.map(value, f)` | Transforma `T` em `U`, preservando `Err(E)`. |
| `result.map_err(value, f)` | Transforma `E` em outro tipo de erro, preservando `Ok(T)`. |
| `result.and_then(value, f)` | Encadeia `f: (T) -> Result[U, E]`. |

`Err` é um valor; não lança uma exceção. Uma função passada a um helper pode
usar `throw`, com as regras usuais de `try/catch`, mas não existe conversão
automática entre esses dois mecanismos.

## Definições próprias

A sintaxe do item 8 também pode ser usada diretamente, sem importar módulos:

```lia
type Maybe<T> = Some(T) | None
type Response<T, E> = Success(T) | Failure(E)
```

Essas declarações são tipos de soma nominais, equivalentes a `enum` com as
mesmas variantes; não são aliases transparentes de outro tipo. Tipos e padrões
seguem as regras documentadas em [ENUMS.md](ENUMS.md). Um `match` precisa cobrir
todas as variantes, incluindo combinações de padrões aninhados. Dois tipos com
variantes de mesmo nome exigem qualificação, como `Maybe.Some` e `Option.Some`.

## Imports, cópia e análises

`option` e `result` são módulos reservados, embutidos no compilador. Não exigem
arquivos `.lia` no diretório do programa e só ficam disponíveis por import
explícito. Importar ambos expõe `Option` e `Result`, com seus construtores, mas
mantém os helpers qualificados para evitar colisões como `map` e `unwrap_or`.
Para escolher um alias de helper, use `import option.{Option, unwrap_or}`.
Arquivos locais com esses nomes de módulo não substituem as implementações
embutidas.

Os helpers são escritos em Lia, não acrescentam primitivas nativas ao runtime.
Eles passam pelas mesmas verificações de tipos, ciclos, terminação e
complexidade, inclusive quando recebem closures ou funções em payloads.
Artefatos `.liac` contêm as declarações resolvidas e podem executar sem os
arquivos de biblioteca.

Valores opcionais e resultados podem ser copiados, retornados, armazenados em
listas ou enviados a rotinas via `spawn`, desde que seus tipos sejam copiáveis.
Por exemplo, `Option[Ref[Int]]` não pode atravessar `spawn`, mesmo em `None`.
Callbacks capturados continuam sujeitos às regras de [CLOSURES.md](CLOSURES.md).

A variante `None` de `Option` é distinta do marcador interno que o runtime usa
quando não há sinal ou memória de loop. Essas leituras não são convertidas
implicitamente em `Option[T]`.

Não há operador postfixo `?`, notação `T?`, desembrulhamento que ignore os casos
de erro nem propagação automática de `Err` nesta implementação. Use os padrões
ou os helpers tipados.

Exemplo executável: [19_option_result.lia](programs/19_option_result.lia).
