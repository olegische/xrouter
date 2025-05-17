package operation_setting

import "one-api/setting/config"

type GeneralSetting struct {
	DocsLink            string `json:"docs_link"`
	PingIntervalEnabled bool   `json:"ping_interval_enabled"`
	PingIntervalSeconds int    `json:"ping_interval_seconds"`
}

// Default configuration
var generalSetting = GeneralSetting{
	DocsLink:            "https://docs.newapi.pro",
	PingIntervalEnabled: false,
	PingIntervalSeconds: 60,
}

func init() {
	// Register to global configuration manager
	config.GlobalConfig.Register("general_setting", &generalSetting)
}

func GetGeneralSetting() *GeneralSetting {
	return &generalSetting
}
