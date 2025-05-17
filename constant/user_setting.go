package constant

var (
	UserSettingNotifyType            = "notify_type"                    // QuotaWarningType Quota warning type
	UserSettingQuotaWarningThreshold = "quota_warning_threshold"        // QuotaWarningThreshold Quota warning threshold
	UserSettingWebhookUrl            = "webhook_url"                    // WebhookUrl webhook address
	UserSettingWebhookSecret         = "webhook_secret"                 // WebhookSecret webhook secret key
	UserSettingNotificationEmail     = "notification_email"             // NotificationEmail Notification email address
	UserAcceptUnsetRatioModel        = "accept_unset_model_ratio_model" // AcceptUnsetRatioModel Whether to accept models with unset prices
)

var (
	NotifyTypeEmail   = "email"   // Email email
	NotifyTypeWebhook = "webhook" // Webhook
)
