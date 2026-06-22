typedef struct {
    void (*free)(uint64_t handle);
    uint64_t (*clone)(uint64_t handle);
} BoltFFICallbackVTablePrefix;

static const BoltFFICallbackVTablePrefix *boltffi_jni_callback_vtable_prefix(const BoltFFICallbackHandle *callback) {
    return callback == NULL ? NULL : (const BoltFFICallbackVTablePrefix *)callback->vtable;
}

static void boltffi_jni_release_callback_value(BoltFFICallbackHandle callback) {
    const BoltFFICallbackVTablePrefix *vtable = boltffi_jni_callback_vtable_prefix(&callback);
    if (callback.handle != 0 && vtable != NULL && vtable->free != NULL) {
        vtable->free(callback.handle);
    }
}

static jlong boltffi_jni_callback_handle_new_owned(JNIEnv *env, BoltFFICallbackHandle callback) {
    if (callback.handle == 0 || callback.vtable == NULL) {
        return 0;
    }
    BoltFFICallbackHandle *stored_callback = (BoltFFICallbackHandle *)malloc(sizeof(BoltFFICallbackHandle));
    if (stored_callback == NULL) {
        boltffi_jni_release_callback_value(callback);
        boltffi_jni_throw_runtime(env, "failed to allocate BoltFFI callback handle");
        return 0;
    }
    *stored_callback = callback;
    return (jlong)(uintptr_t)stored_callback;
}
