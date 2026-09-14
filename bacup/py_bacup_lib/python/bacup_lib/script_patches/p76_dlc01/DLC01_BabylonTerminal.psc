Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || ChoicesRemainingActorValue == None || playerRef.GetValue(ChoicesRemainingActorValue) <= 0.0
		Return
	EndIf
	Form reward = None
	If auiMenuItemID == OptionGivePerkToken
		reward = PerkLeveledItem
	ElseIf auiMenuItemID == OptionRevealEnemies
		reward = RevealEnemiesOnMapPotion
	ElseIf auiMenuItemID == OptionGiveNukeCode
		reward = NukeCodeLeveledItem
	ElseIf auiMenuItemID == OptionGiveAid
		reward = AidLeveledItem
	ElseIf auiMenuItemID == OptionGiveGun
		reward = GunLeveledItem
	EndIf
	If reward != None
		playerRef.AddItem(reward, 1, False)
		playerRef.ModValue(ChoicesRemainingActorValue, -1.0)
		Int index = 0
		While index < OptionActorValues.Length
			If OptionActorValues[index] != None
				playerRef.SetValue(OptionActorValues[index], 0.0)
			EndIf
			index += 1
		EndWhile
	EndIf
EndEvent
