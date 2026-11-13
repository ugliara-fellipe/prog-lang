
3. Tipagem genérica (paramétrica)
**Explanation:** Permite funções reutilizáveis sobre diferentes tipos sem perder a segurança de cópia por valor.
**Example:** `func map<T, U>(list: List[T], f: (T) -> U) -> List[U] { … }`

4. Algebraic Data Types (enum / sum types)
**Explanation:** Define tipos discriminados que o compilador verifica exhaustividade.
**Example:** `enum Result { Ok(Int), Err(String) }`

5. Inferência de tipos
**Explanation:** Reduz verbosidade; o compilador ainda verifica a estrutura de chamadas.
**Example:** `let x = 42   // x é Int`

6. Funções de ordem superior
**Explanation:** Funções podem receber ou retornar outras funções, mantendo a cópia por valor.
**Example:** `func filter<T>(list: List[T], pred: (T) -> Bool) -> List[T] { … }`

7. Closures (captura por valor)
**Explanation:** Captura apenas valores copiados, evitando referências compartilhadas.
**Example:** `let inc = |x: Int| -> Int { x + 1 }`

8. Tipos opcionais / Result
**Explanation:** Representam ausência ou erro de forma segura, sem `null`.
**Example:** `type Maybe<T> = Some(T) | None`

9. Tail‑call optimization (TCO)
**Explanation:** Garante recursões estruturais sem consumo de pilha.
**Example:** `func fact(n: Int, acc: Int) -> Int { if n == 0 { acc } else { tailcall fact(n-1, n*acc) } }`

10. Módulos / pacotes
**Explanation:** Organização em namespaces, carregados antes da análise de chamadas, preservando o DAG.
**Example:** `import math.lia`

11. Constantes de tempo de compilação
**Explanation:** Valores avaliados em compile‑time para limites de loops ou tamanhos de arena.
**Example:** `const MAX_ITER = 1000`

12. Anotações de recursos (budget)
**Explanation:** Declara limites de memória ou passos para rotinas; o runtime valida antes de executar.
**Example:** `routine Heavy() @budget(memory=256KB, steps=500) { … }`

13. Pattern guards
**Explanation:** Extensão do `match` com condições booleanas adicionais.
**Example:** `match x { Some(v) if v > 0 => …, _ => … }`

14. Operadores de pipe (`|>`, `<|`, `>>>`, `->>`)
**Explanation:** `|>` é o padrão recomendado para a maioria das transformações lineares; `<|` pode ser usado como atalho sintático para leitura de trás‑para‑frente; `>>>` e `->>` são úteis para compor funções sem execução imediata, criando pipelines reutilizáveis.
**Example:** `data |> transform |> serialize`

15. Doc‑strings / comentários estruturados
**Explanation:** Geração automática de documentação.
**Example:** `/// Soma dois inteiros`

16. Interoperabilidade FFI
**Explanation:** Chama funções externas de forma controlada, copiando argumentos.
**Example:** `extern "C" fn external_sum(a: Int, b: Int) -> Int;`

17. REPL / modo script
**Explanation:** Execução interativa de rotinas isoladas.
**Example:** `> let x = 5` |


18. descreva a sintaxe de lia dando exemplos de cada estrutura faça um md para isso e uma pagina no site

