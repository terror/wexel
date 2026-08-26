(component
  (type $host-answer-type (func (result u32)))
  (import "host-answer" (func $host-answer (type $host-answer-type)))

  (core func $host-answer-core (canon lower (func $host-answer)))

  (core module $module
    (import "host" "host-answer" (func $host-answer (result i32)))

    (func (export "answer") (result i32)
      call $host-answer
    )
  )

  (core instance $host
    (export "host-answer" (func $host-answer-core))
  )

  (core instance $instance
    (instantiate $module (with "host" (instance $host)))
  )

  (func (export "answer") (result u32)
    (canon lift (core func $instance "answer"))
  )
)
