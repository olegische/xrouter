-- Token Bucket Rate Limiter
-- KEYS[1]: Rate limiter unique identifier
-- ARGV[1]: Requested token count (usually 1)
-- ARGV[2]: Token generation rate (per second)
-- ARGV[3]: Bucket capacity

local key = KEYS[1]
local requested = tonumber(ARGV[1])
local rate = tonumber(ARGV[2])
local capacity = tonumber(ARGV[3])

-- Get current time (Redis server time)
local now = redis.call('TIME')
local nowInSeconds = tonumber(now[1])

-- Get bucket status
local bucket = redis.call('HMGET', key, 'tokens', 'last_time')
local tokens = tonumber(bucket[1])
local last_time = tonumber(bucket[2])

-- Initialize bucket (first request or expired)
if not tokens or not last_time then
    tokens = capacity
    last_time = nowInSeconds
else
    -- Calculate new tokens
    local elapsed = nowInSeconds - last_time
    local add_tokens = elapsed * rate
    tokens = math.min(capacity, tokens + add_tokens)
    last_time = nowInSeconds
end

-- Determine if request is allowed
local allowed = false
if tokens >= requested then
    tokens = tokens - requested
    allowed = true
end

---- Update bucket status and set expiration time
redis.call('HMSET', key, 'tokens', tokens, 'last_time', last_time)
--redis.call('EXPIRE', key, math.ceil(capacity / rate) + 60) -- Appropriately extend expiration time

return allowed and 1 or 0
