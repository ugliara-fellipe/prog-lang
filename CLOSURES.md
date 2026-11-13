# Closures com captura por valor

Uma closure tem parâmetros e retorno explicitamente tipados:

```lia
func adder(base: Int) -> (Int) -> Int {
    |x: Int| -> Int { base + x }
}
routine Worker(f: (Int) -> Int) { f(5) }
routine main() {
    let offset = 10
    let inc = |x: Int| -> Int { offset + x }
    let copy = inc
    let offset = 20
    // inc e copy continuam usando offset = 10.
    let result = adder(3)(4)
    spawn Worker(copy)
}
```

Use `|| -> Int { 42 }` para uma closure sem parâmetros. A última expressão
é o resultado, com as mesmas regras de retorno de uma função nomeada.
Também são permitidos `match`, `try/catch`, exceções e laços limitados. Uma
closure não pode executar `spawn`, `terminate`, `message` ou `memory`: seu
corpo possui contexto de função, mesmo quando criado dentro de uma rotina
ou loop.

## Capturas e escopos

Ao avaliar a expressão de closure, o runtime copia somente as variáveis livres
locais usadas pelo corpo. Parâmetros e bindings locais não são capturas.
Funções nomeadas, construtores e endereços globais de loops continuam sendo
resolvidos nos registros do programa. Uma variável local que sombreia uma
função nomeada é capturada como qualquer outro valor local.

O ambiente capturado não muda quando um nome externo é redeclarado. Cada
invocação recebe um novo ambiente local. Closures retornadas continuam válidas
após o fim da chamada que as criou; closures aninhadas também podem copiar os
parâmetros da invocação externa. Copiar uma closure copia seu ambiente, sem
compartilhar estado mutável.

Referências não podem ser capturadas, inclusive quando aparecem dentro de
listas, structs ou enums. Uma referência que existe no escopo mas não é usada
pelo corpo não é capturada. É possível copiar explicitamente o valor apontado
para uma variável de tipo copiável antes de criar a closure.

A prova de copiabilidade precisa ser estática. Como ainda não há bounds
`Copyable` para parâmetros genéricos arbitrários, capturar diretamente um valor
`T` de uma definição genérica é rejeitado. Capturar uma função com assinatura
conhecida, inclusive uma assinatura que usa parâmetros de tipo, é aceito.

## Chamadas e análises

Closures possuem os mesmos tipos função que funções nomeadas. Elas podem ser
chamadas diretamente, imediatamente após a criação ou após um retorno; podem
ser passadas a funções genéricas, guardadas em listas ou variantes de enum e
copiadas para argumentos de rotinas através de `spawn`.

O verificador confere assinaturas, retornos, padrões e escopos também nos corpos
das closures. O binding de uma variável ainda não existe durante seu próprio
inicializador: uma closure não pode capturar a si mesma por esse binding.

A análise de fluxo dá uma identidade a cada expressão de closure e propaga os
possíveis destinos do seu ambiente capturado e dos argumentos até um ponto
fixo. Instâncias diferentes da mesma expressão são unidas conservadoramente
na análise; no runtime, cada instância mantém seu próprio ambiente.
Essa aproximação pode rejeitar encadeamentos finitos criados repetidamente pela
mesma fábrica de closures, quando não consegue distingui-los de um ciclo.

Os corpos participam das verificações de ciclos e terminação. Ciclos indiretos
por closures são rejeitados, inclusive quando uma referência de função passa
por uma captura ou um enum. A complexidade de uma chamada inclui o custo dos
possíveis corpos de closure, mantendo o modelo de custo já usado pela Lia.

Imports qualificam referências globais dentro das closures, preservando seus
bindings lexicais. Artefatos `.liac` preservam a sintaxe e são revalidados antes
da execução. Falhas de avaliação permanecem distintas de exceções lançadas.

Inferência de parâmetros sem anotações, captura por referência, auto-recursão
anônima e bounds genéricos não fazem parte desta sintaxe.

As chamadas em posição de retorno também utilizam o trampolim de
[TAILCALL.md](TAILCALL.md), incluindo `tailcall` explícito nos corpos.

Exemplo completo: [18_closures.lia](programs/18_closures.lia).
