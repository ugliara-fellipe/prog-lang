# Pendências

A numeração original foi mantida para preservar as referências entre documentos.

## 12. Anotações de recursos (budget)

**Explanation:** Declara limites de memória ou passos para rotinas; o runtime valida antes de executar.
**Example:** `routine Heavy() @budget(memory=256KB, steps=500) { … }`

## 13. Pattern guards

**Explanation:** Extensão do `match` com condições booleanas adicionais.
**Example:** `match x { Some(v) if v > 0 => …, _ => … }`

## 14. Operadores de pipe (`|>`, `<|`, `>>>`, `->>`)

**Explanation:** `|>` é o padrão recomendado para a maioria das transformações lineares; `<|` pode ser usado como atalho sintático para leitura de trás‑para‑frente; `>>>` e `->>` são úteis para compor funções sem execução imediata, criando pipelines reutilizáveis.
**Example:** `data |> transform |> serialize`

## 16. Interoperabilidade FFI

**Explanation:** Chama funções externas de forma controlada, copiando argumentos.
**Example:** `extern "C" fn external_sum(a: Int, b: Int) -> Int;`

## 17. REPL interativo

**Explanation:** Execução interativa de rotinas isoladas. A execução de arquivos pelo CLI já está disponível.
**Example:** `> let x = 5`

## 19. Completar a referência de sintaxe

**Explanation:** Já existem uma visão geral em [README.md](README.md) e o [guia no site](docs/guia.html). Falta consolidar uma referência em Markdown com exemplos de cada estrutura e atualizar o guia para cobrir funções de ordem superior.
