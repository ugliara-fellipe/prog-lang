# Pendências

A numeração original foi mantida para preservar as referências entre documentos.

## 8. Tipos opcionais / Result

**Explanation:** Representam ausência ou erro de forma segura, sem `null`.
**Example:** `type Maybe<T> = Some(T) | None`

## 9. Tail‑call optimization (TCO)

**Explanation:** Garante recursões estruturais sem consumo de pilha.
**Example:** `func fact(n: Int, acc: Int) -> Int { if n == 0 { acc } else { tailcall fact(n-1, n*acc) } }`

## 11. Constantes de tempo de compilação

**Explanation:** Valores avaliados em compile‑time para limites de loops ou tamanhos de arena.
**Example:** `const MAX_ITER = 1000`

## 12. Anotações de recursos (budget)

**Explanation:** Declara limites de memória ou passos para rotinas; o runtime valida antes de executar.
**Example:** `routine Heavy() @budget(memory=256KB, steps=500) { … }`

## 13. Pattern guards

**Explanation:** Extensão do `match` com condições booleanas adicionais.
**Example:** `match x { Some(v) if v > 0 => …, _ => … }`

## 14. Operadores de pipe (`|>`, `<|`, `>>>`, `->>`)

**Explanation:** `|>` é o padrão recomendado para a maioria das transformações lineares; `<|` pode ser usado como atalho sintático para leitura de trás‑para‑frente; `>>>` e `->>` são úteis para compor funções sem execução imediata, criando pipelines reutilizáveis.
**Example:** `data |> transform |> serialize`

## 15. Doc‑strings / comentários estruturados

**Explanation:** Geração automática de documentação.
**Example:** `/// Soma dois inteiros`

## 16. Interoperabilidade FFI

**Explanation:** Chama funções externas de forma controlada, copiando argumentos.
**Example:** `extern "C" fn external_sum(a: Int, b: Int) -> Int;`

## 17. REPL interativo

**Explanation:** Execução interativa de rotinas isoladas. A execução de arquivos pelo CLI já está disponível.
**Example:** `> let x = 5`

## 18. Completar a referência de sintaxe

**Explanation:** Já existem uma visão geral em [README.md](README.md) e o [guia no site](docs/guia.html). Falta consolidar uma referência em Markdown com exemplos de cada estrutura e atualizar o guia para cobrir funções de ordem superior.
