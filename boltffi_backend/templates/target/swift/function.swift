{{ function.documentation() }}public func {{ function.name() }}({% for parameter in function.parameters() %}{{ parameter.signature() }}{% if !loop.last %}, {% endif %}{% endfor %}){{ function.returns().signature() }} {
{{ function.returns().call_body(function.symbol(), function.parameters()) }}
}
