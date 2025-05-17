package model_setting

import (
	"one-api/setting/config"
)

type GlobalSettings struct {
	PassThroughRequestEnabled bool `json:"pass_through_request_enabled"`
}

// Default configuration
var defaultOpenaiSettings = GlobalSettings{
	PassThroughRequestEnabled: false,
}

// Global instance
var globalSettings = defaultOpenaiSettings

func init() {
	// Register to global configuration manager
	config.GlobalConfig.Register("global", &globalSettings)
}

func GetGlobalSettings() *GlobalSettings {
	return &globalSettings
}
