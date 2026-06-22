    {{ method.c_return_type }} result = {0};
    if (!boltffi_jni_read_record(env, __boltffi_return_array, (uintptr_t)sizeof(result), &result)) {
        (*env)->DeleteLocalRef(env, __boltffi_return_array);
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
        return {{ method.failure_value }};
    }
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
