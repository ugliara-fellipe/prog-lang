1. adicione ao loop a palavra reservada memory para persistir e recuperar valores de uma execução para a proxima, uma memoria entra as execuções de cada loop , cada loop tem sua propria memoria e ao terminar sua memoria desaparece com ele


2. descreva a sintaxe de lia dando exemplos de cada estrutura faça um md para isso e uma pagina no site


3. Incorporar recursos adicionais ao Lia

| Index | Feature | Explanation | Example |
|------|---------|-------------|---------|
| 1 | Tipagem genérica (paramétrica) | Permite funções reutilizáveis sobre diferentes tipos sem perder a segurança de cópia por valor. | `func map<T, U>(list: List[T], f: (T) -> U) -> List[U] { … }` |
| 2 | Algebraic Data Types (enum / sum types) | Define tipos discriminados que o compilador verifica exhaustividade. | `enum Result { Ok(Int), Err(String) }` |
| 3 | Inferência de tipos | Reduz verbosidade; o compilador ainda verifica a estrutura de chamadas. | `let x = 42   // x é Int` |
| 4 | Funções de ordem superior | Funções podem receber ou retornar outras funções, mantendo a cópia por valor. | `func filter<T>(list: List[T], pred: (T) -> Bool) -> List[T] { … }` |
| 5 | Closures (captura por valor) | Captura apenas valores copiados, evitando referências compartilhadas. | `let inc = |x: Int| -> Int { x + 1 }` |
| 6 | Tipos opcionais / Result | Representam ausência ou erro de forma segura, sem `null`. | `type Maybe<T> = Some(T) | None` |
| 7 | Tail‑call optimization (TCO) | Garante recursões estruturais sem consumo de pilha. | `func fact(n: Int, acc: Int) -> Int { if n == 0 { acc } else { tailcall fact(n-1, n*acc) } }` |
| 8 | Módulos / pacotes | Organização em namespaces, carregados antes da análise de chamadas, preservando o DAG. | `import math.lia` |
| 9 | Constantes de tempo de compilação | Valores avaliados em compile‑time para limites de loops ou tamanhos de arena. | `const MAX_ITER = 1000` |
|10| Anotações de recursos (budget) | Declara limites de memória ou passos para rotinas; o runtime valida antes de executar. | `routine Heavy() @budget(memory=256KB, steps=500) { … }` |
|11| Pattern guards | Extensão do `match` com condições booleanas adicionais. | `match x { Some(v) if v > 0 => …, _ => … }` |
|12| Operadores de pipe (`|>`, `<|`, `>>>`, `->>`) | `|>` é o padrão recomendado para a maioria das transformações lineares; `<|` pode ser usado como atalho sintático para leitura de trás‑para‑frente; `>>>` e `->>` são úteis para compor funções sem execução imediata, criando pipelines reutilizáveis. | `data |> transform |> serialize` |
|13| Doc‑strings / comentários estruturados | Geração automática de documentação. | `/// Soma dois inteiros` |
|14| Interoperabilidade FFI | Chama funções externas de forma controlada, copiando argumentos. | `extern "C" fn external_sum(a: Int, b: Int) -> Int;` |
|15| REPL / modo script | Execução interativa de rotinas isoladas. | `> let x = 5` |
