# {{ page.title }}

{% for paragraph in page.intro -%}
{{ paragraph.raw }}

{% endfor -%}
## Sources

{% for source in page.sources -%}
- `{{ source.section }}`: [{{ source.title }}]({{ source.href }})
{% endfor %}
## Checks

| id | severity | section | what it flags | reference |
| --- | --- | --- | --- | --- |
{% for check in page.checks -%}
| `{{ check.id }}`{% if check.sampled %} †{% endif %} | {{ check.severity }} | {% match check.section %}{% when Some(section) %}`{{ section }}`{% when None %}(section of the skipped check){% endmatch %} | {{ check.description.raw }} | {% for reference in check.references %}{% if !loop.first %}, {% endif %}<{{ reference }}>{% else %}(same as the skipped check){% endfor %} |
{% endfor -%}
{% if !page.retired.is_empty() %}
## Retired ids

| id | retired | reason |
| --- | --- | --- |
{% for check in page.retired -%}
| `{{ check.id }}` | {{ check.retired }} | {{ check.reason.raw }} |
{% endfor -%}
{% endif -%}
