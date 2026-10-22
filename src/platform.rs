#[cfg(target_os = "android")]
pub fn obtener_portapapeles_jni() -> Option<String> {
    use jni::objects::JObject;

    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let raw_context = ctx.context() as *mut jni::sys::_jobject;
    let activity = unsafe { JObject::from_raw(raw_context) };

    let clipboard_service_str = env.new_string("clipboard").ok()?;
    let clipboard_mgr = env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[(&clipboard_service_str).into()],
        )
        .ok()?
        .l()
        .ok()?;

    if clipboard_mgr.is_null() {
        return None;
    }

    let primary_clip = env
        .call_method(
            &clipboard_mgr,
            "getPrimaryClip",
            "()Landroid/content/ClipData;",
            &[],
        )
        .ok()?
        .l()
        .ok()?;

    if primary_clip.is_null() {
        return None;
    }

    let item = env
        .call_method(
            &primary_clip,
            "getItemAt",
            "(I)Landroid/content/ClipData$Item;",
            &[0.into()],
        )
        .ok()?
        .l()
        .ok()?;

    if item.is_null() {
        return None;
    }

    let char_seq = env
        .call_method(
            &item,
            "coerceToText",
            "(Landroid/content/Context;)Ljava/lang/CharSequence;",
            &[(&activity).into()],
        )
        .ok()?
        .l()
        .ok()?;

    if char_seq.is_null() {
        return None;
    }

    let string_obj = env
        .call_method(&char_seq, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let rust_str: String = env.get_string((&string_obj).into()).ok()?.into();
    Some(rust_str)
}
