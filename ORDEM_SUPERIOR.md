# Modelo para funções de ordem superior na Lia

Este documento registra o modelo implementado para o item 6 de
[TODO.md](TODO.md). O compilador e o runtime aceitam funções nomeadas sem
captura; os testes verificam fluxo, tipos, execução, terminação e custo.

O núcleo inicial compreende funções **nomeadas e sem captura**. Elas podem ser
passadas, retornadas, copiadas e chamadas. Esse modelo foi estendido a closures
com captura por valor, descritas em [CLOSURES.md](CLOSURES.md). A tipagem genérica está descrita em [GENERICOS.md](GENERICOS.md).

## Tipos e chamadas

Um tipo `(T1, ..., Tn) -> R` representa uma função que recebe `n` valores dos
tipos indicados e produz um valor de tipo `R`. Se `Γ` é o ambiente de tipos do
ponto do programa, a regra de chamada é:

```text
Γ ⊢ f : (T1, ..., Tn) -> R     Γ ⊢ ai : Ti para cada i
──────────────────────────────────────────────────────────
Γ ⊢ f(a1, ..., an) : R
```

A regra vale para funções nomeadas, variáveis, parâmetros e expressões que
retornam funções. A implementação deve diagnosticar quantidade e tipos
incompatíveis em chamadas aninhadas, argumentos, inicializações, retornos e
`spawn`. Conversões já permitidas pela linguagem precisam ser definidas de forma
consistente; não se deve presumir que tipos desconhecidos sejam compatíveis.

Cada declaração introduz seu próprio ambiente. Ramos de `match`, blocos de
`try/catch` e laços recebem escopos locais, incluindo associações feitas por
padrões. Um retorno declarado deve ser verificado em todos os caminhos normais;
um caminho que lança uma exceção não constitui retorno normal. A verificação de
exaustividade deve ser conservadora: não considerar que um conjunto de padrões
cobre todos os valores sem justificativa.

## Possíveis destinos: interpretação abstrata

Considere `F` o conjunto finito das funções nomeadas do programa já resolvido,
incluindo os pacotes importados. Para cada ponto relevante de uma expressão,
variável, parâmetro e retorno, a análise mantém um conjunto abstrato de
destinos em `P(F)`, o conjunto das partes de `F`. A ordem é inclusão, a
combinação de caminhos é união e o valor inicial é o conjunto vazio.

Exemplo:

```lia
func dobro(x: Int) -> Int { x * 2 }
func escolher() -> (Int) -> Int { dobro }
func usar() -> Int {
    let f = escolher()
    f(3)
}
```

As restrições devem produzir `destinos(retorno de escolher) = {dobro}` e
`destinos(f) = {dobro}`. Na passagem de um valor de função como argumento, seus
possíveis destinos fluem para o parâmetro correspondente. Nos retornos, os
destinos fluem para o resultado da chamada. Em `match` e `try/catch`, os
resultados possíveis dos caminhos são combinados por união. Listas e outros
contêineres aceitos precisam também propagar os destinos de seus elementos,
inclusive através de padrões e laços.

As restrições são aplicadas até um **ponto fixo**: uma rodada que não acrescente
novos destinos. Como `F` é finito e as operações são monotônicas, esse cálculo
termina. Sua obrigação de correção é conservadora: todo destino concreto
possível deve aparecer no conjunto abstrato. Podem ocorrer falsos positivos,
isto é, destinos incluídos que nunca aparecem em execução.

O conjunto vazio não deve representar silenciosamente um destino desconhecido:
ele pode significar que ainda não houve propagação ou que não há valor possível.
A implementação precisa representar separadamente casos desconhecidos ou não
suportados e rejeitá-los quando não conseguir demonstrar que a chamada é
segura. Compatibilidade de assinatura, sozinha, não demonstra o fluxo de um
valor; pode ser usada como aproximação conservadora se nenhum destino possível
for omitido.

## Grafo de chamadas e terminação

Depois da resolução de pacotes e do cálculo de destinos, uma chamada indireta
gera uma aresta para **cada** função do seu conjunto abstrato. O mesmo resultado
deve ser compartilhado por validação de ciclos, terminação e complexidade;
análises independentes podem discordar e deixar chamadas sem verificação.

Para o subconjunto inicial, rejeitar ciclos indiretos e destinos não resolvidos
é uma política conservadora. Chamadas diretas recursivas continuam sujeitas à
verificação de decréscimo estrutural da linguagem. `spawn` cria uma rotina
assíncrona: seu alvo não é uma chamada síncrona da rotina atual, mas avaliar o
alvo e seus argumentos pode executar funções síncronas que devem ser analisadas.

Essa política pode rejeitar programas que de fato terminam. Um possível avanço
posterior é **size-change termination**, que analisa a diminuição de argumentos
ao longo de sequências de chamadas; aceitá-lo exige uma prova e implementação
próprias, não apenas a presença de um padrão de construtor.

## Complexidade

Para uma chamada com conjunto de destinos `D`, uma composição conservadora é:

```text
custo(chamada) = custo(avaliar alvo e argumentos)
               + máximo { custo(f) | f ∈ D }
```

Em notação assintótica, a soma pode ser representada pelo máximo quando isso
for válido para as classes consideradas. O custo de cada destino deve incluir
suas chamadas e a composição com laços ou recursão estrutural. Uma chamada com
destino desconhecido não pode receber custo `O(1)` por padrão. A análise deve
ser independente da ordem textual das declarações e considerar também as
expressões avaliadas em argumentos de `spawn`, coleções e alvos de `match`.

## Semântica de execução

Um valor de função sem captura pode ser representado pela identidade de uma
função nomeada registrada no programa resolvido. Copiar esse valor copia a
identidade, sem compartilhar estado capturado. Para chamar uma função, o
runtime avalia o alvo e os argumentos, associa os valores aos parâmetros em um
novo ambiente e executa seu corpo. A mesma regra deve valer para uma função
recebida, retornada ou guardada em variável.

Ao passar argumentos em `spawn`, o runtime cria uma cópia por valor de cada
argumento permitido, inclusive de identidades de função. O verificador deve
rejeitar referências e estruturas não copiáveis conforme as regras da Lia.
Falhas ao avaliar o alvo ou argumentos precisam ser propagadas e reportadas;
não devem ser convertidas em `None`. Exceções lançadas devem permanecer
distintas de falhas de avaliação e respeitar os escopos de `try/catch`.

## Critério de conclusão

A rejeição provisória `UnsupportedFunctionValue` foi removida após integrar as
análises e a execução. Os testes cobrem a
passagem, retorno, cópia e chamada de funções, chamadas retornadas, `spawn`,
escopos e padrões, pacotes, terminação e custo, além de rejeições por assinatura
incorreta, valor não copiável, destino desconhecido e ciclo indireto. Os programas
existentes também fazem parte das regressões.

## Sintaxe disponível e limites

```lia
func dobro(x: Int) -> Int { x * 2 }
func aplicar(f: (Int) -> Int, x: Int) -> Int { f(x) }
func escolher() -> (Int) -> Int { dobro }
routine Worker(f: (Int) -> Int) { aplicar(f, 5) }
routine main() {
    let original: (Int) -> Int = dobro
    let copia = original
    let resultado = escolher()(3)
    spawn Worker(copia)
}
```

Também são aceitas referências qualificadas, como `pacote.funcao`, e listas
homogêneas, como `List[(Int) -> Int]`. Os retornos de funções utilizadas como
valores devem possuir assinatura conhecida. Use funções nomeadas e anotações
explícitas para os parâmetros e o retorno; a sintaxe e as regras de captura das closures estão em [CLOSURES.md](CLOSURES.md). A tipagem paramétrica está
descrita em [GENERICOS.md](GENERICOS.md).

`spawn` tem como alvo uma rotina ou um loop nomeado. Valores de função são
argumentos copiáveis dessas rotinas. Declarações `let` podem receber valores de
função; redeclarar um nome cria outro binding e preserva as cópias anteriores.
A linguagem ainda não possui uma instrução geral de reatribuição de variável.

A implementação compartilhada está em `src/semantic/flow.rs`. Ela aproxima
conteúdos de listas por união, sem distinguir índices, e é insensível ao contexto
das chamadas. Para parâmetros de função de declarações sem entradas conhecidas,
a análise usa todas as funções nomeadas com assinatura compatível. Isso pode
rejeitar programas seguros por excesso de aproximação. Leituras dinâmicas cujo
destino de chamada não é conhecido são rejeitadas. Nenhuma dessas aproximações
autoriza omitir destinos possíveis.

O binding de `catch error` ainda não tem um tipo de exceção declarado. Portanto,
usá-lo diretamente como callback é rejeitado pelo verificador de tipos;
retornar ou chamar uma função de fallback com assinatura conhecida é aceito.

A API `ComplexityAnalyzer::try_analyze` reporta falhas na resolução ou ciclos;
`analyze` é a interface de conveniência para programas já validados. O modelo de
custo conserva as classes assintóticas já utilizadas pela Lia, incluindo a
aproximação existente de recursão estrutural; não é uma implementação de AARA.

Exemplo completo: [15_higher_order.lia](programs/15_higher_order.lia).

## Referências

- [Control-flow analysis of function calls and returns by abstract interpretation](https://www.sciencedirect.com/science/article/pii/S0890540111001829).
- [The Size-Change Termination Principle for Constructor Based Languages](https://pierre-hyvernat.apps.math.cnrs.fr/data/Files/sct.pdf).
- [Static determination of quantitative resource usage for higher-order programs](https://researchportal.hw.ac.uk/en/publications/static-determination-of-quantitative-resource-usage-for-higher-or/).
