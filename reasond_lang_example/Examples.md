
```
a := 1 // var
b := 2i32 // var
c = 3i32 // const
a += b // add assign
#c { assert(a = c) }
```

---

```
std.out.auto_flush .= Off
s = "Hello"
print(s)
```

stdout >> `Hello`

---

```
#id std.*
#c.assert(out.auto_flush = AtLineWarp)
#f main() : i32 {
    out.print(args().iter().map(fmt.identify).join(", "))
    0
}
```

stdin << `Hello, I'm fine`

stdout >> `"Hello,", "I'm", "fine"`

---

Slow code
```
#id std.*
arr: [u32]+ = []
loop * input(1..100) {
    arr += rand()
}
arr.remove(-1)
out.print(arr)
```

Fast code
```
#id std.*
out.print([{ rand<u32>() }, * (input(1..100) - 1)])
```

---

Nullable Pointer and NonZero Number

```
ptr : ()*? = 0
non_zero : i64!0 = 1
non_zero -= 1 // Compile error
```

```

```