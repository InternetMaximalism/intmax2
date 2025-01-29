package config

import (
	"fmt"
	"os"
)

type Config struct {
	RedisHost     string
	RedisPort     string
	RedisPassword string
	ServerPort    string
}

func LoadConfig() (*Config, error) {
	config := &Config{}

	config.RedisHost = os.Getenv("REDIS_HOST")
	if config.RedisHost == "" {
		config.RedisHost = "localhost"
	}

	config.RedisPort = os.Getenv("REDIS_PORT")
	if config.RedisPort == "" {
		config.RedisPort = "6379"
	}

	config.RedisPassword = os.Getenv("REDIS_PASSWORD")
	if config.RedisPassword == "" {
		config.RedisPassword = "password"
	}

	config.ServerPort = os.Getenv("PORT")
	if config.ServerPort == "" {
		config.ServerPort = "8080"
	}

	return config, nil
}

func (c *Config) GetRedisAddr() string {
	return fmt.Sprintf("%s:%s", c.RedisHost, c.RedisPort)
}
