(component
  (core module $module
    (table 1 funcref)

    (func (export "grow") (result i32)
      ref.null func
      i32.const 1
      table.grow
    )
  )

  (core instance $instance (instantiate $module))

  (func (export "grow") (result s32)
    (canon lift (core func $instance "grow"))
  )
)
