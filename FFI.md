# Interoperabilidade C

Lia declara funções externas com assinatura tipada e ABI C:

```lia
extern "C" fn add(a: Int, b: Int) -> Int from "./libops.so";
extern "C" fn sum(a: Int, b: Int) -> Int from "./libops.so" as "add" cost "O(1)";
```

`fn` e `func` são aceitos depois de `extern "C"`. O ponto e
vírgula termina a declaração. Não há corpo nem parâmetros de tipo.

`from` indica um caminho de biblioteca compartilhada. Caminhos
relativos são resolvidos a partir do arquivo-fonte que contém a declaração,
inclusive em pacotes com `mod.lia`. O artefato guarda o caminho absoluto;
ao distribuir um artefato, a biblioteca deve estar disponível nesse caminho.

`as` escolhe o símbolo C. Sem ele, o nome original da função é usado,
mesmo quando a função é importada com um nome qualificado de pacote.
Sem `from`, a busca ocorre nos símbolos disponíveis no processo:

```lia
extern "C" fn absolute(value: Int) -> Int as "llabs";
```

## ABI e cópia por valor

| Tipo Lia | Tipo C |
| --- | --- |
| `Int` | `int64_t`, inteiro de 64 bits com sinal |
| `Bool` | `bool` / `_Bool`, retorno limitado a 0 ou 1 |
| `String` em argumento | `char *` ou `const char *`, cópia UTF-8 terminada por NUL |
| `String` em retorno | `const char *`, emprestado pelo código C durante a chamada |
| `Unit` em retorno | `void` |

O retorno omitido significa `Unit`. Até 32 argumentos de aridade fixa
são aceitos, incluindo combinações desses tipos. Não há varargs, ponteiros
Lia, referências, callbacks C, structs, listas, dicionários ou enums na ABI.
Para transportar dados estruturados, uma aplicação pode converter JSON para
`String` explicitamente e usar um wrapper C com esse contrato.

Cada argumento `String` recebe um buffer privado. O código C pode modificar
essa cópia dentro dos limites de seu tamanho original, mantendo a terminação
por NUL, sem alterar o valor Lia original. Não deve liberar, realocar nem guardar esse ponteiro
depois de retornar.

Um retorno `String` deve apontar para bytes UTF-8 válidos, terminados por
NUL e acessíveis durante a chamada. A biblioteca continua responsável pela
memória original; Lia copia o texto e não libera esse ponteiro. Não retorne
um buffer temporário da stack nem transfira a Lia a responsabilidade por um
buffer alocado com `malloc`.

Textos de entrada com NUL interno são rejeitados. Retornos nulos,
UTF-8 inválido e booleanos fora de 0/1 geram falha FFI.

## Exemplo de biblioteca

```c
#include <stdint.h>
#include <stdbool.h>

int64_t add(int64_t a, int64_t b) { return a + b; }
bool positive(int64_t n) { return n > 0; }
const char *greeting(void) { return "hello"; }
```

No Linux, compile esse arquivo como biblioteca compartilhada:

```sh
cc -shared -fPIC ops.c -o libops.so
```

```lia
import std

extern "C" fn add(a: Int, b: Int) -> Int from "./libops.so";
extern "C" fn positive(n: Int) -> Bool from "./libops.so";
extern "C" fn greeting() -> String from "./libops.so";

routine main() {
    std.log(add(2, 3), positive(3), greeting())
}
```

A assinatura deve coincidir com a implementação C real. `Int` não
significa `int` de 32 bits. O carregador encontra símbolos, mas não
consegue inspecionar seus tipos C.

## Execução, análises e confiança

Compilar, consultar complexidade e gerar documentação não carregam as
bibliotecas do usuário nem executam seus inicializadores. A carga é adiada
até a primeira chamada. Erros de carga e de símbolo são reportados como
falhas de avaliação FFI, sem substituir argumentos ou resultados por
`None`. Essas falhas não são exceções Lia de `throw`.

Os handles de bibliotecas ficam abertos até o fim do processo, preservando
seu estado. Chamadas feitas por Lia são serializadas, incluindo a cópia de
retornos, para permitir buffers estáticos de retorno. Threads criadas pela
própria biblioteca continuam sob responsabilidade de seu autor.

A declaração `extern` é um contrato de terminação do programador: o
compilador assume que a função externa sempre termina. Não inspeciona a
implementação C nem tenta demonstrar essa propriedade. Uma biblioteca que
bloqueie indefinidamente ou viole memória quebra o contrato; assinaturas e
pré-condições do código nativo continuam sob responsabilidade do programador.

## Declarar custo

Acrescente `cost "O(1)"`, `cost "O(N)"` ou `cost "O(N^k)"` antes
do ponto e vírgula, depois de `from` e `as`, quando presentes:

```lia
extern "C" fn absolute(n: Int) -> Int as "llabs" cost "O(1)";
extern "C" fn transform(text: String) -> String
    from "./libops.so" as "transform" cost "O(N)";
```

O expoente `k` deve ser um inteiro positivo representável em 32 bits.
A anotação expressa um limite superior para a chamada completa, incluindo
o código C e a cópia de argumentos e resultados. O compilador confia nesse
limite, não o mede nem o verifica. Cabe ao programador considerar o tamanho
dos dados, a cópia de strings e as pré-condições da biblioteca.

A análise propaga o custo declarado para chamadores, loops, callbacks e
composições. Os custos seguem o modelo polinomial existente. Para uma classe
não representada, use um limite polinomial conservador válido ou omita a
anotação. Custos inválidos são diagnosticados; não são aproximados em silêncio.

Sem `cost`, a chamada continua assumida como terminante, mas sua
complexidade permanece `unknown (foreign code)`. Artefatos antigos sem a
anotação mantêm esse comportamento.

## Inventariar as fronteiras externas

```sh
lia externs program.lia
lia externs program.liac
lia externs program.lia --all
lia externs library.lia --entry Worker --json
```

O comando valida o programa e lista os bindings externos potencialmente
usados. Não carrega bibliotecas nem executa funções. Por padrão, começa em
`routine main` e percorre chamadas diretas, indiretas, closures, composições,
`spawn` e destinos de sinais. A análise é conservadora: braços condicionais
podem aparecer mesmo quando uma execução particular não os percorre.

A saída inclui nome Lia, ABI, biblioteca, símbolo C, arquivo e linha da
declaração, chamadores, custo declarado e efetivo e a terminação assumida
por contrato. `UNDECLARED` destaca custos ausentes. Esses dados permitem
localizar as afirmações do programador que sustentam a análise.

`--all` inclui também declarações não alcançadas a partir da entrada
selecionada e indica seu estado. `--entry nome` seleciona outra função,
rotina ou loop. Em bibliotecas sem `main`, todas as entradas públicas
e símbolos externos exportados são considerados. `--json` fornece um
inventário estruturado. Cada nome Lia aparece separadamente, mesmo quando
dois aliases apontam para o mesmo símbolo C.

A compilação grava a localização e o contrato no artefato. Artefatos antigos
sem localização exibem essa ausência; recompilá-los registra a origem.

Valores de função externos podem ser passados, retornados, copiados, compostos
e enviados a rotinas com `spawn`, usando as mesmas verificações estáticas de
assinatura de outras funções. As restrições sobre referências em `spawn`
permanecem. Nenhum ponteiro de callback Lia é enviado ao C.

No REPL, declare a função e execute a chamada em entradas separadas.
O estado nativo e seus efeitos não são revertidos por um erro ou por
`:reset`. O caminho relativo usa o diretório-base da sessão.

## Compilar o compilador

A implementação usa `libffi` e o carregador POSIX de bibliotecas. São
necessários um compilador C, `ar`, `pkg-config` e os headers e bibliotecas
de desenvolvimento de `libffi`. A plataforma validada é Linux.
O build consulta `pkg-config` e compila o bridge em
[native/ffi.c](native/ffi.c); `CC` e `AR` podem escolher os executáveis
da toolchain.

Veja [programs/27_ffi.lia](programs/27_ffi.lia) para execução com o runtime C
do processo e [tests/ffi.rs](tests/ffi.rs) para testes que compilam bibliotecas
C reais, incluindo assinaturas mistas, strings, pacotes, artefatos e erros.
