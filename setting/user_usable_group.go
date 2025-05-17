package setting

import (
	"encoding/json"
	"one-api/common"
)

var userUsableGroups = map[string]string{
	"default": "Default Group",
	"vip":     "VIP Group",
}

func GetUserUsableGroupsCopy() map[string]string {
	copyUserUsableGroups := make(map[string]string)
	for k, v := range userUsableGroups {
		copyUserUsableGroups[k] = v
	}
	return copyUserUsableGroups
}

func UserUsableGroups2JSONString() string {
	jsonBytes, err := json.Marshal(userUsableGroups)
	if err != nil {
		common.SysError("error marshalling user groups: " + err.Error())
	}
	return string(jsonBytes)
}

func UpdateUserUsableGroupsByJSONString(jsonStr string) error {
	userUsableGroups = make(map[string]string)
	return json.Unmarshal([]byte(jsonStr), &userUsableGroups)
}

func GetUserUsableGroups(userGroup string) map[string]string {
	groupsCopy := GetUserUsableGroupsCopy()
	if userGroup == "" {
		if _, ok := groupsCopy["default"]; !ok {
			groupsCopy["default"] = "default"
		}
	}
	// If userGroup is not in UserUsableGroups, return UserUsableGroups + userGroup
	if _, ok := groupsCopy[userGroup]; !ok {
		groupsCopy[userGroup] = "User Group"
	}
	// If userGroup is in UserUsableGroups, return UserUsableGroups
	return groupsCopy
}

func GroupInUserUsableGroups(groupName string) bool {
	_, ok := userUsableGroups[groupName]
	return ok
}
