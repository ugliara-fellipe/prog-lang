# Constantes de tempo de compilação

Use `const NOME = expressão`, com anotação opcional de tipo. A declaração
pode aparecer no topo do arquivo ou dentro de funções, rotinas, loops,
closures e blocos de `match` ou `try/catch`.

```lia
import std

const BASE = 250
const MAX_ITER: Int = BASE * 4
const LABEL = "contador"
const READY: Bool = MAX_ITER > 0

routine main() {
    const LOCAL = MAX_ITER / BASE
    for i in LOCAL {
        std.log(LABEL, i)
    }
}
```

As expressões são avaliadas durante a compilação, depois da resolução de
imports e antes das verificações de tipos, chamadas e terminação.
O artefato `.liac` contém dados literais tipados, sem os cálculos dos
inicializadores. O runtime apenas materializa cópias desses dados.
A execução direta de um arquivo passa pela mesma compilação.

## Expressões e tipos aceitos

Um inicializador pode conter:

- Literais `Int`, `String`, `True` e `False`.
- Outras constantes, inclusive nomes qualificados de pacotes.
- Listas e dicionários de dados constantes.
- Aritmética inteira: `+`, `-`, `*` e `/`.
- Comparações inteiras: `<`, `<=`, `>` e `>=`.
- Igualdade e desigualdade entre dados do mesmo tipo: `==` e `!=`.
- Parênteses e consultas `type NOME` de constantes visíveis.

Os tipos aceitos são `Int`, `String`, `Bool`, `Json`,
`List[T]` e `Dict[T]`, com elementos que também sejam dados constantes.
Coleções heterogêneas compatíveis com JSON inferem elementos `Json`.
Coleções vazias sem contexto usam `Json` como tipo de elemento; uma
anotação pode determinar outro tipo, como `const IDS: List[Int] = []`.

```lia
const LIMITS: Dict[Int] = {iterations: 1000, batch: 20}
const LABELS: List[String] = ["small", "large"]
const CONFIG: Dict[Json] = {enabled: True, labels: LABELS, retries: 3}
const KIND = type CONFIG
```

Uma anotação conserva seu significado nos usos da constante:
`const VALUE: Json = 1` continua tendo tipo `Json`. Para obter
um `Int`, consulte `json.as_int(VALUE)`, como faria com uma variável
`Json`.

Chamadas de funções, closures, referências, consultas por índice,
`message`, `memory`, parâmetros e variáveis `let` não são
inicializadores constantes, mesmo quando parecem produzir um resultado
fixo. Não há execução de funções arbitrárias durante a compilação.
`Json` aceita os dados literais acima; `json.parse` e `json.null`
são chamadas de runtime e não podem aparecer em inicializadores `const`.

Inteiros têm 64 bits com sinal. Overflow e divisão por zero geram erro
de compilação, inclusive em constantes não utilizadas. Divisão inteira
trunca em direção a zero. Para expressar um negativo, use `0 - valor`,
seguindo os operadores atuais da linguagem.

## Dependências, imports e escopos

Constantes no topo podem referenciar declarações posteriores. O
compilador resolve suas dependências e rejeita ciclos, com indicação
dos nomes envolvidos.

```lia
const DOUBLE = BASE * 2
const BASE = 500
```

Imports completos e seletivos incluem constantes:

```lia
import settings.{MAX_ITER}
const LOCAL_LIMIT = MAX_ITER / 2
const OTHER_LIMIT = settings.MAX_ITER
```

Constantes locais só podem depender de constantes já visíveis naquele
ponto e de constantes globais. Elas respeitam os limites dos blocos.
Parâmetros, padrões, variáveis de iteração e declarações locais podem
sombrear nomes, como ocorre com `let`. Um nome sombreado por um
valor de runtime não pode ser usado para inicializar outra constante.
Declarações globais conflitantes e os nomes booleanos `True` e
`False` são rejeitados.

Os dados seguem a cópia por valor da linguagem. Closures podem utilizar
constantes de escopos externos; `spawn` pode recebê-las. Atualizar um
dicionário com `dict.set` produz outro valor e conserva a constante.

## Limites de iteração

`for i in limite { ... }` aceita um `Int` não negativo. O índice
percorre `0` até `limite - 1`; limite zero não executa o corpo.
O limite é avaliado uma vez. A iteração é incremental, sem construir
uma lista proporcional ao limite. Limites negativos geram falha de
avaliação.

Esse caminho funciona em funções, rotinas e loops persistentes.
`for` sobre listas ou dicionários continua disponível.
Limites inteiros conhecidos diretamente como literais ou constantes
têm fator de iteração `O(1)` na análise assintótica, pois independem
da entrada de runtime. Limites inteiros dinâmicos mantêm fator
`O(N)`. Chamadas e o custo do corpo continuam sendo analisados.

Índices de intervalos inteiros não são tratados como payloads
indutivos para liberar recursão estrutural. As regras de terminação
das chamadas recursivas permanecem aplicáveis.

O tamanho das arenas ainda é administrado pelo runtime. A sintaxe
de configuração e fiscalização de recursos pertence ao item 12
(`budget`); `const` fornece os valores para consumidores que
aceitam limites.

Veja [programs/22_constants.lia](programs/22_constants.lia) para um
programa executável e [tests/constants.rs](tests/constants.rs) para
os casos aceitos e rejeitados.
