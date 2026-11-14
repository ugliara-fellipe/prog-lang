# REPL interativo

Inicie uma sessão com:

```sh
cargo run -- repl
# Ou use o executável:
lia repl
lia repl --base-dir programs
```

O diretório-base é o diretório atual por padrão. Imports procuram pacotes
nesse diretório. Os pacotes embutidos continuam disponíveis; `std` usa
uma cópia embutida quando não há um arquivo correspondente no diretório-base.
Um `std.lia` local continua sujeito à validação dos primitivos certificados.

```text
lia> let x = 5
lia> x + 2
7
lia> :type x
Int
lia> :bindings
x: Int = 5
lia> func increment(n: Int) -> Int {
...>     n + 1
...> }
lia> x |> increment
6
lia> :quit
```

## Entradas e comandos

Expressões no nível da sessão exibem seu resultado. Declarações `let`
e `const`, imports, funções, tipos, rotinas e loops também são aceitos.
Blocos `match`, `try/catch` e `for` seguem a sintaxe normal.
Parênteses, colchetes, chaves e literais abertos permitem continuar a
entrada em outras linhas. Doc-strings `///` acompanham a declaração seguinte.

Cada entrada deve ser um conjunto de declarações de programa ou um
conjunto de instruções e expressões. Não misture, numa única entrada,
uma declaração de função com uma chamada depois de sua chave final;
envie a chamada na entrada seguinte.

| Comando | Efeito |
| --- | --- |
| `:help` | Lista os comandos |
| `:bindings` | Lista variáveis da sessão, seus tipos e valores, em ordem de nome |
| `:type EXPRESSÃO` | Consulta o tipo sem executar a expressão |
| `:reset` | Remove bindings, declarações, imports e cache de pacotes |
| `:abort` | Descarta uma entrada multilinha ainda incompleta |
| `:quit` ou `:q` | Encerra a sessão |
| EOF | Encerra; uma entrada incompleta é diagnosticada |
| Ctrl-C | Encerra a sessão, inclusive durante uma execução |

Constantes são declarações de programa; podem ser consultadas por nome
ou com `:type`, mas não aparecem na lista de variáveis de `:bindings`.
Funções nomeadas não capturam variáveis `let` da sessão. Para capturar
esses valores, use closures, como no restante da linguagem.

## Estado, validação e isolamento

O histórico fornece contexto às análises de tipos, destinos de chamadas,
ciclos e terminação. Só as instruções da entrada nova são executadas.
Consultar uma variável ou declarar outra função não repete prints,
chamadas ou spawns anteriores.

Bindings guardam cópias de valores entre entradas, incluindo closures,
dicionários, enums e funções compostas. Referências não copiáveis não
podem escapar de uma entrada em bindings persistentes. Elas podem ser
usadas localmente em uma rotina, respeitando as regras normais de
confinamento e `spawn`.

Uma entrada com erro de sintaxe, validação ou avaliação não altera os
bindings, declarações e imports já confirmados. A sessão relata o erro
e aceita a próxima entrada. Efeitos externos já realizados durante uma
tentativa, como prints e trabalho de rotinas despachadas, não são desfeitos.

Declarações nomeadas não podem ser redefinidas na mesma sessão; use
`:reset` para substituí-las. Variáveis podem ser sombreadas com outro
`let`, preservando as capturas antigas das closures.

Pacotes importados ficam fixados na sessão. Alterar um arquivo no disco
não muda funções nem tipos de valores existentes. Use `:reset` para
carregar versões novas. Imports que falham não contaminam o cache.

## Rotinas e entrada por pipe

Declare e execute rotinas explicitamente com `spawn`:

```text
lia> import std
lia> routine Worker(n: Int) { std.log("worker", n) }
lia> spawn Worker(3)
worker 3
```

Declarar `routine main()` não a executa automaticamente no REPL.
Uma entrada espera o trabalho despachado terminar antes de aceitar a
próxima. Loops persistentes devem alcançar `terminate` ou a sessão
deve ser encerrada com Ctrl-C. O router, a memória dos loops e a arena
de execução são privados de cada entrada; não há atores ativos em
segundo plano entre entradas confirmadas.

O REPL também aceita stdin redirecionado, sem banner ou prompts:

```sh
printf 'let x = 5\nx + 2\n:quit\n' | lia repl
```

Resultados vão para stdout e diagnósticos para stderr. Erros de entradas
são recuperáveis: a sessão continua e seu encerramento normal não funciona
como um status de validação de arquivo. Para compilação com status de
erro, use os comandos de compilação existentes.

Os testes de integração estão em [tests/repl.rs](tests/repl.rs).
