Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	MoMMasterQuestScript masterScript = Game.GetFormFromFile(0x00345D51, "SeventySix.esm") as MoMMasterQuestScript
	If masterScript != None
		masterScript.InitializeLocalEquipmentTracking()
	EndIf
	Int userIndex = -1
	If auiMenuItemID == 2
		userIndex = 1
	ElseIf auiMenuItemID == 3
		userIndex = 0
	ElseIf auiMenuItemID == 4
		userIndex = 2
	ElseIf auiMenuItemID == 5
		userIndex = 3
	EndIf

	If userIndex >= 0 && userIndex < CryptosUserIDs.Length && CryptosUserIDTokenLabels.Length >= 3
		CryptosUserID selectedUser = CryptosUserIDs[userIndex]
		akTerminalRef.AddTextReplacementData(CryptosUserIDTokenLabels[0], selectedUser.UserIDWithBar)
		akTerminalRef.AddTextReplacementData(CryptosUserIDTokenLabels[1], selectedUser.UserIDWithoutBar)
		akTerminalRef.AddTextReplacementData(CryptosUserIDTokenLabels[2], selectedUser.UserIDNoSpaces)
	EndIf
EndEvent
