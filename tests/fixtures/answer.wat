(component
  (core module $module
    (func (export "answer") (result i32)
      i32.const 42
    )
  )

  (core instance $instance (instantiate $module))

  (func (export "answer") (result u32)
    (canon lift (core func $instance "answer"))
  )
)
