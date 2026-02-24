package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var (
	cfgFile   string
	apiURL    string
	authToken string
	output    string
)

// rootCmd represents the base command when called without any subcommands
var rootCmd = &cobra.Command{
	Use:   "valence",
	Short: "CLI for Valence v2 knowledge engine",
	Long: `Valence CLI provides command-line access to the Valence v2 knowledge engine.
It wraps the HTTP API for inserting triples, querying the graph, running
maintenance operations, and more.`,
}

// Execute adds all child commands to the root command and sets flags appropriately.
func Execute() {
	err := rootCmd.Execute()
	if err != nil {
		os.Exit(1)
	}
}

func init() {
	cobra.OnInitialize(initConfig)

	// Global persistent flags
	rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default is $HOME/.config/valence/config.toml)")
	rootCmd.PersistentFlags().StringVar(&apiURL, "api-url", "http://localhost:8421", "Valence API base URL")
	rootCmd.PersistentFlags().StringVar(&authToken, "token", "", "Authentication token")
	rootCmd.PersistentFlags().StringVarP(&output, "output", "o", "table", "Output format (json, table, plain)")

	// Bind flags to viper
	viper.BindPFlag("api-url", rootCmd.PersistentFlags().Lookup("api-url"))
	viper.BindPFlag("token", rootCmd.PersistentFlags().Lookup("token"))
	viper.BindPFlag("output", rootCmd.PersistentFlags().Lookup("output"))
}

// initConfig reads in config file and ENV variables if set
func initConfig() {
	if cfgFile != "" {
		// Use config file from the flag
		viper.SetConfigFile(cfgFile)
	} else {
		// Find home directory
		home, err := os.UserHomeDir()
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}

		// Search config in $HOME/.config/valence directory
		viper.AddConfigPath(home + "/.config/valence")
		viper.SetConfigType("toml")
		viper.SetConfigName("config")
	}

	// Environment variables override config file
	viper.SetEnvPrefix("VALENCE")
	viper.AutomaticEnv()

	// If a config file is found, read it in
	if err := viper.ReadInConfig(); err == nil {
		fmt.Fprintln(os.Stderr, "Using config file:", viper.ConfigFileUsed())
	}
}
