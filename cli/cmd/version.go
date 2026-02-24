package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
)

var (
	version   = "0.1.0"
	buildDate = "unknown"
	gitCommit = "unknown"
)

var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Print version information",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("valence-cli version %s\n", version)
		fmt.Printf("  build date: %s\n", buildDate)
		fmt.Printf("  git commit: %s\n", gitCommit)
	},
}

func init() {
	rootCmd.AddCommand(versionCmd)
}
