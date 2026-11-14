out.reas


<!-- if self.used > 0 {
  if self.auto_flush == AutoFlush.On {
    self.inner.flush()
  } else if self.auto_flush == AutoFlush.AtLineWarp {
    if self.used % T.meta.bytes != 0 {
      self.inner.flush()
    }
  }
} -->
```
enum AutoFlush {Off, AtLineWarp, On}
auto_flush = AutoFlush.AtLineWarp
#t BufWriter<T, I> (buf: heap[T], used: usz, inner: I, auto_flush: AutoFlush) {
  #f @new(buffer_size: usz, inner: I, auto_flush: AutoFlush) {
    Self(buf: heap[T](buffer_size), used: 0, inner, auto_flush)
  }
  #f @flush(self) {
    #req(self.used > 0) |> #ret
    self.auto_flush == AutoFlush.On
    -> self.inner.flush()
    |> self.auto_flush == AutoFlush.AtLineWarp
      -> self.used % T.meta.bytes != 0
          -> self.inner.flush()
  }
  #f @write(self, data: &[T]) {
    self.used += data.len()
    self.used > self.buf.len()
    -> self.inner.write(self.buf[..=self.used])
       self.used = 0
    data.len() > self.buf.len()
    -> self.inner.write(data)
    |> copy(data, self.buf[self.used..])
  }
}
```

ptr.reas
```
#t &#<T>
// pointer is not null by default, add ? before type to make it nullable
#t *#<T> (addr: [!set] usz) {
  #f @"+"(self, off: isz) {
    self.addr + off * T.meta.bytes
    self
  }
  #f @"+="
  #f @"-"(self, off: isz) {
    self.addr - off * T.meta.bytes
    self
  }
  #f @"-="
  #f(off: isz) self[off] -> &T {
    (self + off).*
  }  #f usz(self) { self.addr }
  #f ()(self) {}
}

#t heap<T, Allocator = #default_alloc> (ptr: *T, alloc: Allocator) {
  #t @new(value: T) {
    alloc := Allocator()
    Self(ptr: alloc.alloc(value), alloc)
  }
  #f @drop(self) {
    alloc.drop(self.ptr)
  }
}
```

array.reas
```
#t<T> [T] (ptr: *T, size: usz) {
  #f(off: isz) self[off] {
    self.*
    off < 0
    -> #req(-off <= size) |> #throw
       off += size
    |> #req(off < size) |> #throw
    self.ptr[off]
  }
}

#t<T> heap[T]<Allocator = #default_alloc> (ptr: heap<[T], Allocator>) {
  #f(off: isz) self[off]  { self.ptr[off] }
}

#t<T> dyn[T] (buf: ?heap[T], used: usz) {
  #f @new() { Self(buf: null, used: 0) }
  #f @new(arr: [T]) { Self(buf: heap[T](arr), used: arr.len()) }
  #f(off: isz) self[off] { self.buf[off] }
  #f reserve(&self, size: usz) {
    self.*
    #req(buf.size < size) |> #ret
    new_buf := heap[T](size)
    copy(buf, new_buf)
    drop(buf)
    buf .= new_buf
  }
  #f add_back(self, value: T | ()->T) {
    self.*
    used + 1 > buf.size -> reserve(buf.size * 2)
    buf[used] = value
    used += 1
  }
}
```

tuple.reas
```
#t<T...> (T...) (#c { T #each.enumerate.str_code(i, I) f"{i}: {I}" }) {

}

```