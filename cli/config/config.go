package config

import (
	"github.com/spf13/viper"
)

// Config holds CLI configuration
type Config struct {
	APIURL    string
	AuthToken string
	Output    string
}

// Load loads configuration from viper
func Load() *Config {
	return &Config{
		APIURL:    viper.GetString("api-url"),
		AuthToken: viper.GetString("token"),
		Output:    viper.GetString("output"),
	}
}
