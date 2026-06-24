{%- if record.empty() %}
object {{ record.name() }} {
    internal fun toByteArray(): ByteArray = ByteArray(0)

    internal fun fromByteArray(bytes: ByteArray): {{ record.name() }} {
        require(bytes.size == 0)
        return {{ record.name() }}
    }
}
{%- else %}
data class {{ record.name() }}(
{%- for field in record.fields() %}
    val {{ field.name() }}: {{ field.ty() }}{% if !loop.last %},{% endif %}
{%- endfor %}
) {
    internal fun toByteArray(): ByteArray {
        val buffer = java.nio.ByteBuffer
            .allocate({{ record.size() }})
            .order(java.nio.ByteOrder.nativeOrder())
{%- for field in record.fields() %}
        {{ field.write() }}
{%- endfor %}
        return buffer.array()
    }

    companion object {
        internal fun fromByteArray(bytes: ByteArray): {{ record.name() }} {
            require(bytes.size == {{ record.size() }})
            val buffer = java.nio.ByteBuffer
                .wrap(bytes)
                .order(java.nio.ByteOrder.nativeOrder())
            return {{ record.name() }}(
{%- for field in record.fields() %}
                {{ field.read() }}{% if !loop.last %},{% endif %}
{%- endfor %}
            )
        }
    }
}
{%- endif %}
