# Tipagem genérica

Funções, rotinas e tipos podem declarar parâmetros de tipo. O verificador
substitui esses parâmetros e confere cada uso; o runtime executa o mesmo corpo
sem gerar cópias por instância. Artefatos `.liac` preservam as declarações.

```lia
func identidade<T>(valor: T) -> T { valor }
type Caixa<T> { valor: T }
func abrir<T>(caixa: Caixa[T]) -> T { caixa.valor }
routine Worker<T>(valor: T) { identidade(valor) }
routine main() {
    let numero = identidade(3)
    let texto = identidade<String>("Lia")
    let copia: (Int) -> Int = identidade
    spawn Worker<Int>(numero)
}
```

A inferência usa os argumentos e, quando disponível, o tipo esperado do
resultado. Também é possível especializar um valor: `identidade<Int>`.
Para callbacks genéricos, uma especialização explícita ou uma variável com tipo
função anotado resolve casos sem informação suficiente para inferência.

Escreva `<` junto ao nome na especialização: `identidade<Int>`. Comparações
ambíguas devem usar espaços: `a < b > c`. Tipos aplicados usam colchetes:
`List[Int]`, `Caixa[String]` e `List[(Int) -> Bool]`.

Um parâmetro `T` é arbitrário no corpo da declaração: `x + 1` não é permitido
quando `x: T`. Ocorrências repetidas do mesmo parâmetro precisam concordar.
Tipos não declarados, quantidades incorretas e parâmetros que não podem ser
inferidos são rejeitados. Não há constraints, bounds ou polimorfismo de ordem
superior; valores de função possuem uma assinatura instanciada.

`std.empty<T>()` e `std.prepend<T>(valor, lista)` possuem tipagem polimórfica no
verificador, permitindo implementar `map<T, U>`. Veja o exemplo executável
[16_generics.lia](../../programs/16_generics.lia).

A cópia por valor e o confinamento de referências permanecem verificados:
instanciar `T` com `Ref[Int]` não permite atravessar `spawn`. A rotina de entrada
`main` não pode declarar parâmetros de tipo. As verificações de fluxo,
terminação e complexidade também percorrem especializações explícitas.
