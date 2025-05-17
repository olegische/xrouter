package operation_setting

import "strings"

const (
	// Web search
	WebSearchHighTierModelPriceLow    = 30.00
	WebSearchHighTierModelPriceMedium = 35.00
	WebSearchHighTierModelPriceHigh   = 50.00
	WebSearchPriceLow                 = 25.00
	WebSearchPriceMedium              = 27.50
	WebSearchPriceHigh                = 30.00
	// File search
	FileSearchPrice = 2.5
)

func GetWebSearchPricePerThousand(modelName string, contextSize string) float64 {
	// Determine model type
	// https://platform.openai.com/docs/pricing Web search price is charged based on model type and search context size
	// gpt-4.1, gpt-4o, or gpt-4o-search-preview are more expensive, gpt-4.1-mini, gpt-4o-mini, gpt-4o-mini-search-preview are cheaper
	isHighTierModel := (strings.HasPrefix(modelName, "gpt-4.1") || strings.HasPrefix(modelName, "gpt-4o")) &&
		!strings.Contains(modelName, "mini")
	// Determine the price corresponding to search context size
	var priceWebSearchPerThousandCalls float64
	switch contextSize {
	case "low":
		if isHighTierModel {
			priceWebSearchPerThousandCalls = WebSearchHighTierModelPriceLow
		} else {
			priceWebSearchPerThousandCalls = WebSearchPriceLow
		}
	case "medium":
		if isHighTierModel {
			priceWebSearchPerThousandCalls = WebSearchHighTierModelPriceMedium
		} else {
			priceWebSearchPerThousandCalls = WebSearchPriceMedium
		}
	case "high":
		if isHighTierModel {
			priceWebSearchPerThousandCalls = WebSearchHighTierModelPriceHigh
		} else {
			priceWebSearchPerThousandCalls = WebSearchPriceHigh
		}
	default:
		// search context size defaults to medium
		if isHighTierModel {
			priceWebSearchPerThousandCalls = WebSearchHighTierModelPriceMedium
		} else {
			priceWebSearchPerThousandCalls = WebSearchPriceMedium
		}
	}
	return priceWebSearchPerThousandCalls
}

func GetFileSearchPricePerThousand() float64 {
	return FileSearchPrice
}
