{{- define "virgo-auth.fullname" -}}
{{ .Release.Name }}
{{- end -}}

{{- define "virgo-auth.labels" -}}
app.kubernetes.io/name: virgo-auth
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{- define "virgo-auth.selector" -}}
app.kubernetes.io/name: virgo-auth
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "virgo-auth.image" -}}
{{ printf "%s:%s" .repository (toString .tag) }}
{{- end -}}
