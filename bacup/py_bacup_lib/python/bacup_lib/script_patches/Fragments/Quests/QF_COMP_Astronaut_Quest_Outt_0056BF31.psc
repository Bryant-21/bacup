Function Fragment_Stage_0100_Item_00()
	Actor player = GetPlayerActor()
	If player && COMP_Astronaut_Outtro_Keycard_ATHENA && player.GetItemCount(COMP_Astronaut_Outtro_Keycard_ATHENA) == 0
		player.AddItem(COMP_Astronaut_Outtro_Keycard_ATHENA, 1, True)
	EndIf
	ObjectReference mapMarker = Alias_MapMarker.GetReference()
	If mapMarker
		mapMarker.Enable()
	EndIf
	DisplayObjectiveOnce(200)
EndFunction

Function Fragment_Stage_0200_Item_00()
	DisplayObjectiveOnce(200)
EndFunction

Function Fragment_Stage_0500_Item_00()
	CompleteObjectiveOnce(200)
	DisplayObjectiveOnce(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
	DisplayObjectiveOnce(1000)
EndFunction

Function Fragment_Stage_2000_Item_00()
	If COMP_Astronaut_Quest_Outtro_CHOICE && !COMP_Astronaut_Quest_Outtro_CHOICE.IsPlaying()
		COMP_Astronaut_Quest_Outtro_CHOICE.Start()
	EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
	CompleteObjectiveOnce(1000)
	DisplayObjectiveOnce(3000)
EndFunction

Function Fragment_Stage_3060_Item_00()
	DisplayObjectiveOnce(3060)
	DisplayObjectiveOnce(3100)
EndFunction

Function Fragment_Stage_3090_Item_00()
	SetChoiceValues(False)
	If COMP_Astronaut_Quest_Outtro_ATHENAShutdown && !COMP_Astronaut_Quest_Outtro_ATHENAShutdown.IsPlaying()
		COMP_Astronaut_Quest_Outtro_ATHENAShutdown.Start()
	EndIf
	If !IsStageDone(4000)
		SetStage(4000)
	EndIf
EndFunction

Function Fragment_Stage_3100_Item_00()
	If COMP_Astronaut_Quest_Outtro_TransferScene && !COMP_Astronaut_Quest_Outtro_TransferScene.IsPlaying()
		COMP_Astronaut_Quest_Outtro_TransferScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_3110_Item_00()
	ObjectReference soundMarker = Alias_TransferSoundMarker.GetReference()
	If QSTMassFusionPowerDown && soundMarker
		QSTMassFusionPowerDown.Play(soundMarker)
	EndIf
EndFunction

Function Fragment_Stage_3190_Item_00()
	SetChoiceValues(True)
EndFunction

Function Fragment_Stage_3197_Item_00()
	ObjectReference assaultron = Alias_Assaultron_Instanced.GetReference()
	If assaultron
		assaultron.Disable()
	EndIf
EndFunction

Function Fragment_Stage_3200_Item_00()
	DisplayObjectiveOnce(3060)
	DisplayObjectiveOnce(3100)
EndFunction

Function Fragment_Stage_3290_Item_00()
	SetChoiceValues(True)
	If !IsStageDone(5000)
		SetStage(5000)
	EndIf
EndFunction

Function Fragment_Stage_3500_Item_00()
	CompleteObjectiveOnce(3500)
EndFunction

Function Fragment_Stage_3510_Item_00()
	SetChoiceValues(False)
	If COMP_Astronaut_Quest_Outtro_ATHENAShutdown && !COMP_Astronaut_Quest_Outtro_ATHENAShutdown.IsPlaying()
		COMP_Astronaut_Quest_Outtro_ATHENAShutdown.Start()
	EndIf
	If !IsStageDone(4000)
		SetStage(4000)
	EndIf
EndFunction

Function Fragment_Stage_3520_Item_00()
	SetChoiceValues(True)
	If COMP_Astronaut_Quest_Outtro_TransferScene && !COMP_Astronaut_Quest_Outtro_TransferScene.IsPlaying()
		COMP_Astronaut_Quest_Outtro_TransferScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_3600_Item_00()
	CompleteObjectiveOnce(3600)
EndFunction

Function Fragment_Stage_4000_Item_00()
	SetChoiceValues(False)
	OpenLocalWrapUp()
EndFunction

Function Fragment_Stage_4100_Item_00()
	CompleteObjectiveOnce(4000)
	DisplayObjectiveOnce(6000)
EndFunction

Function Fragment_Stage_5000_Item_00()
	SetChoiceValues(True)
	OpenLocalWrapUp()
EndFunction

Function Fragment_Stage_5100_Item_00()
	CompleteObjectiveOnce(4000)
	DisplayObjectiveOnce(6000)
EndFunction

Function Fragment_Stage_6000_Item_00()
	CompleteObjectiveOnce(3500)
	CompleteObjectiveOnce(3600)
	CompleteObjectiveOnce(4000)
	DisplayObjectiveOnce(6000)
	ObjectReference mapMarker = Alias_MapMarker.GetReference()
	If mapMarker
		mapMarker.Disable()
	EndIf
EndFunction

Function Fragment_Stage_9100_Item_00()
	CompleteObjectiveOnce(6000)
	Actor player = GetPlayerActor()
	If player && COMP_AV_Astronaut_FinaleComplete
		player.SetValue(COMP_AV_Astronaut_FinaleComplete, 1.0)
	EndIf
	CompanionScript astronaut = Alias_Astronaut.GetActorReference() as CompanionScript
	If astronaut && astronaut.CampQuest && astronaut.CampQuest.IsRunning() && !astronaut.CampQuest.IsStageDone(astronaut.CampQuestCompletionStage)
		astronaut.CampQuest.SetStage(astronaut.CampQuestCompletionStage)
	EndIf
	CompleteQuest()
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
	FailAllObjectives()
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Actor player = GetPlayerActor()
	If player && COMP_Astronaut_Outtro_Keycard_ATHENA && player.GetItemCount(COMP_Astronaut_Outtro_Keycard_ATHENA) > 0
		player.RemoveItem(COMP_Astronaut_Outtro_Keycard_ATHENA, 1, True)
	EndIf
	ObjectReference mapMarker = Alias_MapMarker.GetReference()
	If mapMarker
		mapMarker.Disable()
	EndIf
	Stop()
EndFunction

Actor Function GetPlayerActor()
	Actor player = Alias_Player.GetActorReference()
	If !player
		player = Game.GetPlayer()
		If player
			Alias_Player.ForceRefIfEmpty(player)
		EndIf
	EndIf
	Return player
EndFunction

Function DisplayObjectiveOnce(Int objective)
	If !IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
		SetObjectiveDisplayed(objective)
	EndIf
EndFunction

Function CompleteObjectiveOnce(Int objective)
	If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
		SetObjectiveCompleted(objective)
	EndIf
EndFunction

Function SetChoiceValues(Bool savedAthena)
	Actor player = GetPlayerActor()
	If !player
		Return
	EndIf
	If COMP_AV_Astronaut_Outtro_MadeATHENAChoice
		player.SetValue(COMP_AV_Astronaut_Outtro_MadeATHENAChoice, 1.0)
	EndIf
	If COMP_AV_Astronaut_PlayerChoice_SavedAthena
		If savedAthena
			player.SetValue(COMP_AV_Astronaut_PlayerChoice_SavedAthena, 1.0)
		Else
			player.SetValue(COMP_AV_Astronaut_PlayerChoice_SavedAthena, 0.0)
		EndIf
	EndIf
EndFunction

Function OpenLocalWrapUp()
	CompleteObjectiveOnce(3000)
	CompleteObjectiveOnce(3060)
	CompleteObjectiveOnce(3100)
	DisplayObjectiveOnce(3500)
	DisplayObjectiveOnce(3600)
	DisplayObjectiveOnce(4000)
EndFunction
