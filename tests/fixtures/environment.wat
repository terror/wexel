(component
  (type $environment (instance
    (export "get-arguments" (func (result (list string))))
    (export "get-environment" (func (result (list (tuple string string)))))
    (export "initial-cwd" (func (result (option string))))
  ))

  (import "wasi:cli/environment@0.2.12"
    (instance $environment (type $environment))
  )

  (alias export $environment "get-environment"
    (func $get-environment)
  )

  (core module $allocator
    (memory (export "memory") 1)
    (global $heap (mut i32) (i32.const 8))

    (func (export "realloc")
      (param $old-pointer i32)
      (param $old-size i32)
      (param $alignment i32)
      (param $new-size i32)
      (result i32)
      (local $pointer i32)

      global.get $heap
      local.get $alignment
      i32.const 1
      i32.sub
      i32.add
      local.get $alignment
      i32.const 1
      i32.sub
      i32.const -1
      i32.xor
      i32.and
      local.tee $pointer
      local.get $new-size
      i32.add
      global.set $heap
      local.get $pointer
    )
  )

  (core instance $allocator-instance (instantiate $allocator))
  (alias core export $allocator-instance "memory"
    (core memory $memory)
  )
  (alias core export $allocator-instance "realloc"
    (core func $realloc)
  )

  (core func $lowered
    (canon lower (func $get-environment)
      (memory $memory)
      (realloc $realloc)
    )
  )

  (core instance $lowered-instance
    (export "get-environment" (func $lowered))
  )
  (core module $wrapper
    (import "environment" "get-environment"
      (func $get-environment (param i32))
    )

    (func (export "environment") (result i32)
      i32.const 0
      call $get-environment
      i32.const 0
    )
  )
  (core instance $wrapper-instance
    (instantiate $wrapper
      (with "environment" (instance $lowered-instance))
    )
  )
  (alias core export $wrapper-instance "environment"
    (core func $wrapped)
  )

  (func $lifted (result (list (tuple string string)))
    (canon lift (core func $wrapped)
      (memory $memory)
    )
  )

  (export "environment" (func $lifted))
)
