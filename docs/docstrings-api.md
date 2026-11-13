# API: `23_docstrings.lia`

## Imports

- `import std`

## const `REPEATS`

```lia
const REPEATS: Int = 2
```

Quantidade de execuções deste exemplo.

## type `Point`

```lia
type Point {
    x: Int,
    y: Int
}
```

Coordenadas de um ponto no plano.

### Field `x`

```lia
x: Int
```

Posição horizontal.

### Field `y`

```lia
y: Int
```

Posição vertical.

## type `Lookup`

```lia
type Lookup<T> = Found(T) | Missing
```

Resultado de uma consulta opcional.

### Variant `Found`

```lia
Found(T)
```

Valor encontrado.

### Variant `Missing`

```lia
Missing
```

Nenhum valor disponível.

## func `identity`

```lia
func identity<T>(value: T) -> T
```

Identidade com parâmetro de tipo.

Retorna uma cópia do argumento.

Exemplo:
~~~lia
identity<Int>(3)
~~~

### Parameters

- `value`: `T`

### Returns

`T`

## func `add`

```lia
func add(a: Int, b: Int) -> Int
```

Soma dois inteiros.

- a: primeira parcela.
- b: segunda parcela.

O resultado tem tipo Int.

### Parameters

- `a`: `Int`
- `b`: `Int`

### Returns

`Int`

## loop `Listener`

```lia
loop Listener()
```

Recebe uma mensagem e encerra o ator.

## routine `Worker`

```lia
routine Worker(value: Int)
```

Imprime o valor recebido.

### Parameters

- `value`: `Int`

## routine `main`

```lia
routine main()
```

Executa os exemplos documentados.
