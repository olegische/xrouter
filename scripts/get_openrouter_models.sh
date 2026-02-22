#!/usr/bin/env bash

# Fetch models list from OpenRouter API.
# Spec: GET /api/v1/models
# Docs: https://openrouter.ai/docs/api-reference/models/get-models

set -euo pipefail

BASE_URL="${OPENROUTER_BASE_URL:-https://openrouter.ai/api/v1}"
ENDPOINT="$BASE_URL/models"
API_KEY="${OPENROUTER_API_KEY:-}"

OUTPUT_MODE="ids" # ids | json | providers | popular-providers
CATEGORY=""
SUPPORTED_PARAMETERS=""
USE_RSS=""
USE_RSS_CHAT_LINKS=""

PROVIDERS_FILTER=""
ALL_PROVIDERS=false
CURRENT_ONLY=true
CURATED_POPULAR_PROVIDERS_CSV="${CURATED_POPULAR_PROVIDERS_CSV:-anthropic,openai,google,minimax,moonshotai,deepseek,x-ai,z-ai}"

OPENAI_MIN_VERSION="${OPENAI_MIN_VERSION:-5.3}"
ANTHROPIC_MIN_VERSION="${ANTHROPIC_MIN_VERSION:-4.5}"
GOOGLE_MIN_VERSION="${GOOGLE_MIN_VERSION:-2.5}"
MINIMAX_MIN_VERSION="${MINIMAX_MIN_VERSION:-2.0}"
MOONSHOTAI_MIN_VERSION="${MOONSHOTAI_MIN_VERSION:-2.0}"
XAI_MIN_VERSION="${XAI_MIN_VERSION:-4.0}"
ZAI_MIN_VERSION="${ZAI_MIN_VERSION:-4.5}"

print_help() {
  cat <<'USAGE'
Usage:
  scripts/get_openrouter_models.sh [options]

Options:
  --json                        Print full JSON response
  --ids                         Print only model IDs (default)
  --providers-stats             Print provider -> models count (all providers)
  --popular-providers           Print only curated popular providers with count
  --providers <csv|popular>     Filter by explicit providers, e.g. openai,anthropic,google
  --all-providers               Disable default popular-providers filter
  --no-current-filter           Disable current-models filter
  --category <value>            Pass category query param
  --supported-parameters <val>  Pass supported_parameters query param
  --use-rss <true|false>        Pass use_rss query param
  --use-rss-chat-links <bool>   Pass use_rss_chat_links query param
  -h, --help                    Show this help
USAGE
}

urlencode() {
  local value="$1"
  jq -nr --arg v "$value" '$v|@uri'
}

parse_csv_to_json_array() {
  local csv="$1"
  jq -nc --arg csv "$csv" '
    $csv
    | split(",")
    | map(gsub("^\\s+|\\s+$"; ""))
    | map(select(length > 0))
    | unique
  '
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --json)
      OUTPUT_MODE="json"
      shift
      ;;
    --ids)
      OUTPUT_MODE="ids"
      shift
      ;;
    --providers-stats)
      OUTPUT_MODE="providers"
      shift
      ;;
    --popular-providers)
      OUTPUT_MODE="popular-providers"
      shift
      ;;
    --all-providers)
      ALL_PROVIDERS=true
      shift
      ;;
    --no-current-filter)
      CURRENT_ONLY=false
      shift
      ;;
    --providers)
      PROVIDERS_FILTER="${2:-}"
      shift 2
      ;;
    --category)
      CATEGORY="${2:-}"
      shift 2
      ;;
    --supported-parameters)
      SUPPORTED_PARAMETERS="${2:-}"
      shift 2
      ;;
    --use-rss)
      USE_RSS="${2:-}"
      shift 2
      ;;
    --use-rss-chat-links)
      USE_RSS_CHAT_LINKS="${2:-}"
      shift 2
      ;;
    -h|--help)
      print_help
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      print_help
      exit 1
      ;;
  esac
done

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: curl is required" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "Error: jq is required" >&2
  exit 1
fi

query=()

if [[ -n "$CATEGORY" ]]; then
  query+=("category=$(urlencode "$CATEGORY")")
fi
if [[ -n "$SUPPORTED_PARAMETERS" ]]; then
  query+=("supported_parameters=$(urlencode "$SUPPORTED_PARAMETERS")")
fi
if [[ -n "$USE_RSS" ]]; then
  query+=("use_rss=$(urlencode "$USE_RSS")")
fi
if [[ -n "$USE_RSS_CHAT_LINKS" ]]; then
  query+=("use_rss_chat_links=$(urlencode "$USE_RSS_CHAT_LINKS")")
fi

if [[ ${#query[@]} -gt 0 ]]; then
  ENDPOINT+="?$(IFS='&'; echo "${query[*]}")"
fi

curl_args=(
  -sS
  "$ENDPOINT"
  -H "Accept: application/json"
)

if [[ -n "$API_KEY" ]]; then
  curl_args+=( -H "Authorization: Bearer $API_KEY" )
fi

response="$(curl "${curl_args[@]}")"

provider_stats_filter='[
  .data[]?.id
  | select(type == "string")
  | split("/")[0]
]
| group_by(.)
| map({provider: .[0], count: length})
| sort_by(-.count, .provider)'

provider_stats_json="$(echo "$response" | jq -c "$provider_stats_filter")"
curated_provider_names_json="$(parse_csv_to_json_array "$CURATED_POPULAR_PROVIDERS_CSV")"

explicit_provider_names_json='[]'
if [[ -n "$PROVIDERS_FILTER" ]]; then
  if [[ "$PROVIDERS_FILTER" == "popular" ]]; then
    explicit_provider_names_json="$curated_provider_names_json"
  else
    explicit_provider_names_json="$(parse_csv_to_json_array "$PROVIDERS_FILTER")"
  fi
fi

active_provider_names_json='[]'
if [[ "$explicit_provider_names_json" != '[]' ]]; then
  active_provider_names_json="$explicit_provider_names_json"
elif [[ "$ALL_PROVIDERS" == true ]]; then
  active_provider_names_json='[]'
else
  active_provider_names_json="$curated_provider_names_json"
fi

jq_defs='def parse_ver($s):
  ($s | capture("(?<maj>[0-9]+)(?:\\.(?<min>[0-9]+))?")? // null) as $m
  | if $m == null then null else {maj: ($m.maj | tonumber), min: (($m.min // "0") | tonumber)} end;

def ver_gte($candidate; $required):
  (parse_ver($candidate)) as $c
  | (parse_ver($required)) as $r
  | if $c == null or $r == null then false
    elif $c.maj > $r.maj then true
    elif $c.maj < $r.maj then false
    else $c.min >= $r.min
    end;

def provider_name:
  (.id | split("/")[0]);

def model_name:
  (.id | split("/") | .[1:] | join("/"));

def pass_provider($providers):
  if ($providers | length) == 0 then true
  else (provider_name as $provider | ($providers | index($provider)))
  end;

def pass_current($current_only; $openai_min; $anthropic_min; $google_min; $minimax_min; $moonshotai_min; $xai_min; $zai_min):
  if $current_only then
    provider_name as $provider
    | model_name as $name
      | if $provider == "openai" then
        ($name | test("^gpt-5\\.(2|3)(-|$)"))
      elif $provider == "anthropic" then
        ver_gte($name; $anthropic_min)
      elif $provider == "google" then
        (ver_gte($name; $google_min) and ($name | test("^gemma") | not))
      elif $provider == "minimax" then
        ver_gte($name; $minimax_min)
      elif $provider == "moonshotai" then
        ver_gte($name; $moonshotai_min)
      elif $provider == "deepseek" then
        (
          (
            ($name | test("^deepseek-v"))
            and (($name | test("v3\\.2|v4|v5")))
          )
          or ($name | test("^deepseek-r1"))
        )
        and ($name | test("distill") | not)
      elif $provider == "x-ai" then
        ver_gte($name; $xai_min)
      elif $provider == "z-ai" then
        ($name | test("^glm-(5|4\\.7)"))
      else
        true
      end
  else
    true
  end;'

if [[ "$OUTPUT_MODE" == "providers" ]]; then
  echo "$provider_stats_json" | jq -r '.[] | "\(.provider)\t\(.count)"'
  exit 0
fi

if [[ "$OUTPUT_MODE" == "popular-providers" ]]; then
  echo "$provider_stats_json" | jq -r --argjson providers "$curated_provider_names_json" '
    .[]
    | select(($providers | index(.provider)))
    | "\(.provider)\t\(.count)"
  '
  exit 0
fi

if [[ "$OUTPUT_MODE" == "json" ]]; then
  echo "$response" | jq \
    --argjson providers "$active_provider_names_json" \
    --argjson current_only "$CURRENT_ONLY" \
    --arg openai_min "$OPENAI_MIN_VERSION" \
    --arg anthropic_min "$ANTHROPIC_MIN_VERSION" \
    --arg google_min "$GOOGLE_MIN_VERSION" \
    --arg minimax_min "$MINIMAX_MIN_VERSION" \
    --arg moonshotai_min "$MOONSHOTAI_MIN_VERSION" \
    --arg xai_min "$XAI_MIN_VERSION" \
    --arg zai_min "$ZAI_MIN_VERSION" \
    "$jq_defs
    .data |= [
      .[]
      | select(.id | type == \"string\")
      | select(pass_provider(\$providers))
      | select(pass_current(\$current_only; \$openai_min; \$anthropic_min; \$google_min; \$minimax_min; \$moonshotai_min; \$xai_min; \$zai_min))
    ]"
  exit 0
fi

echo "$response" | jq -r \
  --argjson providers "$active_provider_names_json" \
  --argjson current_only "$CURRENT_ONLY" \
  --arg openai_min "$OPENAI_MIN_VERSION" \
  --arg anthropic_min "$ANTHROPIC_MIN_VERSION" \
  --arg google_min "$GOOGLE_MIN_VERSION" \
  --arg minimax_min "$MINIMAX_MIN_VERSION" \
  --arg moonshotai_min "$MOONSHOTAI_MIN_VERSION" \
  --arg xai_min "$XAI_MIN_VERSION" \
  --arg zai_min "$ZAI_MIN_VERSION" \
  "$jq_defs
  .data[]?
  | select(.id | type == \"string\")
  | select(pass_provider(\$providers))
  | select(pass_current(\$current_only; \$openai_min; \$anthropic_min; \$google_min; \$minimax_min; \$moonshotai_min; \$xai_min; \$zai_min))
  | .id" | sort
