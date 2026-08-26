(component
  (core module $module
    (memory 1)

    (func (export "grow") (result i32)
      i32.const 1
      memory.grow
    )
  )

  (core instance $instance (instantiate $module))

  (func (export "grow") (result s32)
    (canon lift (core func $instance "grow"))
  )
)
