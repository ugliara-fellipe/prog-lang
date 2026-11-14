# Chamadas em cauda

O avaliador executa chamadas em cauda por um trampolim: prepara o destino e os
argumentos, encerra o ambiente da chamada atual e inicia a próxima iteração do
avaliador. A cadeia dessas chamadas não acrescenta frames à pilha nativa.
Isso vale para funções nomeadas, especializações genéricas e closures.

A otimização é automática. O marcador `tailcall` é uma asserção verificada
pelo compilador sobre a posição da chamada, não uma permissão para ignorar
outras verificações.

```lia
func sum(items: List[Int], acc: Int) -> Int {
    match items {
        Nil => acc,
        Cons(head, tail) => tailcall sum(tail, acc + head)
    }
}
```

Remover `tailcall` do exemplo não altera a otimização. O primeiro argumento
recursivo continua sendo o subcomponente `tail`, conforme a prova estrutural.

## Posições aceitas

Uma chamada é elegível quando fornece diretamente o resultado de uma função
ou closure: a última expressão do corpo ou a última expressão de um ramo de
`match`, `try` ou `catch` que fornece esse resultado. Blocos aninhados herdam
essa condição somente quando não há trabalho posterior.

```lia
func apply(f: (Int) -> Int, value: Int) -> Int {
    tailcall f(value)
}
func identity<T>(value: T) -> T { value }
func generic(value: Int) -> Int { tailcall identity<Int>(value) }
```

O marcador é rejeitado em inicializadores, argumentos, operadores, alvos de
`match`, `throw`, coleções e corpos de `for`, `spawn`, rotinas ou loops. Uma
closure criada dentro de uma rotina possui seu próprio contexto de função e
pode usar `tailcall` em seu resultado. Chamadas que deixam instruções posteriores
no mesmo caminho não são elegíveis.

Por exemplo, `1 + count(tail)` não é uma chamada em cauda: a soma ainda precisa
ser executada depois do retorno. `tailcall count(tail) + 1` é rejeitado. Chamadas
feitas para preparar os argumentos ou o destino de outra chamada também deixam
trabalho pendente; o marcador não as transforma em chamadas em cauda.

## Argumentos, capturas e exceções

O destino e todos os argumentos são avaliados, na ordem habitual, usando o
ambiente anterior. Só então o trampolim substitui a chamada. Assim,
`tailcall f(tail, b, a)` troca os dois argumentos sem sobrescrever o valor
necessário para avaliar o segundo.

Closures conservam seus ambientes copiados. Quando uma closure é chamada em
cauda, seu ambiente e os parâmetros formam o próximo escopo local.

Handlers de `try/catch` pendentes no percurso de uma chamada em cauda são
mantidos numa estrutura explícita no heap.
Um `throw` do destino em cauda ainda é tratado pelo handler lexical correto;
um `throw` dentro de um `catch` pode alcançar o handler externo. Falhas de
avaliação continuam sendo erros, não exceções capturáveis.

```lia
func fail(x: Int) -> Int { throw x }
func guarded(x: Int) -> Int {
    try { tailcall fail(x) } catch error { 0 }
}
```

Preservar handlers pode reter escopos e consumir memória no heap. A garantia
refere-se aos frames de chamada do avaliador; não transforma todos os dados,
cópias ou handlers em memória constante. Chamadas fora de posição de cauda
continuam podendo consumir pilha.

## Garantias preservadas

Assinaturas, retornos, copiabilidade, destinos indiretos e ciclos seguem as
mesmas regras de antes. A análise de custo percorre a chamada e seus argumentos.

TCO não é prova de terminação: `tailcall f(xs)` é rejeitado numa autochamada
quando `xs` não diminui. Ciclos indiretos e recursão mútua continuam sujeitos à
restrição do grafo. O marcador não introduz uma prova para decrementos
aritméticos como `n - 1` nem acrescenta uma sintaxe `if`: use os dados indutivos
e padrões já verificados pela Lia.

Imports preservam os destinos e artefatos `.liac` preservam o marcador. A posição
em cauda é revalidada antes da execução de artefatos.

Os testes incluem 2.000 passos de recursão estrutural numa thread com pilha de
256 KiB, com e sem o marcador, e uma cadeia de 3.000 funções distintas, com
transições por closures. Também
verificam chamadas genéricas, closures, argumentos simultâneos, handlers,
assinaturas incorretas e usos fora de posição de cauda.

Exemplo executável: [20_tailcalls.lia](../../programs/20_tailcalls.lia).
