# Pattern guards

Um braço de `match` pode acrescentar uma condição booleana entre o
padrão e `=>`:

```lia

func classify(value: Option[Int]) -> String {
    match value {
        Some(n) if n > 0 => "positive",
        Some(n) if n == 0 => "zero",
        Some(_) => "negative",
        None => "missing"
    }
}
```

A sintaxe é `padrão if expressão => corpo`. O corpo continua aceitando
uma expressão ou um bloco. `if` é uma palavra reservada para guards;
esta implementação não adiciona um comando condicional independente.

## Ordem e escopo

O alvo do `match` é avaliado uma vez. Os braços são examinados na ordem
em que aparecem:

1. Se o padrão não casar, o guard desse braço não é avaliado.
2. Se casar, suas variáveis ficam disponíveis no guard.
3. Com `True`, o corpo executa e a seleção termina.
4. Com `False`, a seleção continua no braço seguinte.

Variáveis associadas pelo padrão pertencem somente ao guard e ao corpo
daquele braço. Elas podem sombrear nomes externos. Uma tentativa cujo
guard retorne `False` não disponibiliza suas variáveis aos próximos
braços nem ao código depois do `match`.

O guard deve ter tipo `Bool`. Inteiros, textos, referências e coleções
não são convertidos implicitamente em condições. O compilador também
verifica tipos e nomes em guards e corpos de braços inalcançáveis.

## Exaustividade e retornos

Braços com guard não contam como cobertura para provar exaustividade,
mesmo quando a condição é literalmente `True`. Garanta a cobertura
com braços sem guard, como `Some(_)` e `None` no exemplo, ou com um
fallback `_ => ...`.

```lia
func positive(n: Int) -> Bool {
    match n {
        x if x > 0 => True,
        _ => False
    }
}
```

A verificação de retornos continua exigindo o tipo declarado em todos
os caminhos normais. Adicionar um guard a um braço antes exaustivo pode
exigir outro braço sem guard.

## Chamadas, closures e falhas

Guards podem chamar funções e valores de função que retornem `Bool`,
incluindo callbacks associados pelo próprio padrão:

```lia

func accepted(value: Option[(Int) -> Bool], n: Int) -> Bool {
    match value {
        Some(test) if test(n) => True,
        _ => False
    }
}
```

Chamadas dentro de guards participam das análises de ciclos, terminação
e complexidade, inclusive chamadas indiretas. Capturas usadas somente
no guard também fazem parte da closure e seguem as regras de cópia por
valor e de confinamento de referências.

Uma falha de avaliação não equivale a `False`. Uma exceção lançada por
uma função chamada no guard propaga até um `catch` aplicável; erros de
avaliação continuam sendo reportados. O runtime não tenta um fallback
depois de uma falha. Efeitos de funções chamadas por um guard, como
mensagens de `std.log`, ocorrem mesmo se o resultado for `False`.

Guards funcionam em funções, rotinas e loops persistentes. O contexto
de `message` e `memory` continua restrito aos loops. As regras de
tipagem existentes desses valores continuam aplicáveis.

O guard não é uma posição de retorno para `tailcall`. Uma chamada no
corpo selecionado ainda pode usar `tailcall` quando o corpo está na
posição final de uma função ou closure. Chamadas dentro de closures
criadas no guard seguem as posições de retorno da própria closure.

Artefatos antigos, sem o campo opcional de guard, continuam aceitos.
Veja [programs/24_pattern_guards.lia](../../programs/24_pattern_guards.lia)
para um exemplo com opcionais, callbacks, dicionários, closures e
`spawn`, e [tests/pattern_guards.rs](../../tests/pattern_guards.rs) para
casos aceitos e rejeitados.
