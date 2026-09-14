Function Fragment_Stage_0001_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
		If playerRef != None
			Alias_Player.ForceRefTo(playerRef)
		EndIf
	EndIf
	If playerRef != None && pBoS03StartedAV != None
		playerRef.SetValue(pBoS03StartedAV, 1.0)
	EndIf
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	If !IsObjectiveDisplayed(100)
		SetObjectiveDisplayed(100, True)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && oBoS03UpdateTransponderPerk != None && !playerRef.HasPerk(oBoS03UpdateTransponderPerk)
		playerRef.AddPerk(oBoS03UpdateTransponderPerk)
	EndIf
	If pBoS03_Transponders != None && !pBoS03_Transponders.IsRunning()
		pBoS03_Transponders.Start()
	EndIf
	If pBoS03_Transponders_01 != None && !pBoS03_Transponders_01.IsPlaying()
		pBoS03_Transponders_01.Start()
	EndIf
	If pBoS03_Transponder_Radio != None && !pBoS03_Transponder_Radio.IsPlaying()
		pBoS03_Transponder_Radio.Start()
	EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
	ObjectReference nextTransponder = Alias_Transponder02.GetReference()
	If nextTransponder != None
		Alias_CurrentTransponder.ForceRefTo(nextTransponder)
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If pBoS03NextTransponderMessage != None
		pBoS03NextTransponderMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
	ObjectReference nextTransponder = Alias_Transponder03.GetReference()
	If nextTransponder != None
		Alias_CurrentTransponder.ForceRefTo(nextTransponder)
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If pBoS03NextTransponderMessage != None
		pBoS03NextTransponderMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
	ObjectReference nextTransponder = Alias_Transponder04.GetReference()
	If nextTransponder != None
		Alias_CurrentTransponder.ForceRefTo(nextTransponder)
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If pBoS03NextTransponderMessage != None
		pBoS03NextTransponderMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
	ObjectReference nextTransponder = Alias_Transponder05.GetReference()
	If nextTransponder != None
		Alias_CurrentTransponder.ForceRefTo(nextTransponder)
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If pBoS03NextTransponderMessage != None
		pBoS03NextTransponderMessage.Show()
	EndIf

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && pBoS03_CheckpointValue != None
		playerRef.SetValue(pBoS03_CheckpointValue, 1.0)
	EndIf
	If pCheckpointMessage != None
		pCheckpointMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True)
	Alias_CurrentTransponder.Clear()

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && oBoS03UpdateTransponderPerk != None && playerRef.HasPerk(oBoS03UpdateTransponderPerk)
		playerRef.RemovePerk(oBoS03UpdateTransponderPerk)
	EndIf
	If pBoS03_Transponder_Radio != None && pBoS03_Transponder_Radio.IsPlaying()
		pBoS03_Transponder_Radio.Stop()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(400, True)
	SetObjectiveDisplayed(500, True)

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None
		Alias_PlayerHasAccess.ForceRefTo(playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(500, True)
	CompleteAllObjectives()

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && pBoS03CompletedAV != None
		playerRef.SetValue(pBoS03CompletedAV, 1.0)
	EndIf
	CompleteQuest()
	If BoS03_TryStartEN01()
		SetStage(700)
	Else
		StartTimer(5.0, 600)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && oBoS03UpdateTransponderPerk != None && playerRef.HasPerk(oBoS03UpdateTransponderPerk)
		playerRef.RemovePerk(oBoS03UpdateTransponderPerk)
	EndIf
	If pBoS03_Transponder_Radio != None && pBoS03_Transponder_Radio.IsPlaying()
		pBoS03_Transponder_Radio.Stop()
	EndIf
	If pBoS03_Transponders_01 != None && pBoS03_Transponders_01.IsPlaying()
		pBoS03_Transponders_01.Stop()
	EndIf
	If pBoS03_Transponders != None && pBoS03_Transponders.IsRunning()
		pBoS03_Transponders.Stop()
	EndIf
	Alias_CurrentTransponder.Clear()
	Stop()
EndFunction

Function Fragment_Stage_9000_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None
		If oBoS03UpdateTransponderPerk != None && playerRef.HasPerk(oBoS03UpdateTransponderPerk)
			playerRef.RemovePerk(oBoS03UpdateTransponderPerk)
		EndIf
		If pBoS03StartedAV != None
			playerRef.SetValue(pBoS03StartedAV, 0.0)
		EndIf
		If pBoS03_CheckpointValue != None
			playerRef.SetValue(pBoS03_CheckpointValue, 0.0)
		EndIf
	EndIf
	If pBoS03_Transponder_Radio != None && pBoS03_Transponder_Radio.IsPlaying()
		pBoS03_Transponder_Radio.Stop()
	EndIf
	If pBoS03_Transponders_01 != None && pBoS03_Transponders_01.IsPlaying()
		pBoS03_Transponders_01.Stop()
	EndIf
	If pBoS03_Transponders != None && pBoS03_Transponders.IsRunning()
		pBoS03_Transponders.Stop()
	EndIf
	Alias_CurrentTransponder.Clear()
EndFunction

Bool Function BoS03_TryStartEN01()
	Quest en01Quest = Game.GetFormFromFile(0x000649C5, "SeventySix.esm") as Quest
	If en01Quest != None && (en01Quest.IsRunning() || en01Quest.IsCompleted())
		Return True
	EndIf

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef == None || pEN01_MiscQuestStartKeyword == None
		Return False
	EndIf

	Bool accepted = pEN01_MiscQuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	Return accepted || (en01Quest != None && (en01Quest.IsRunning() || en01Quest.IsCompleted()))
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != 600 || !GetStageDone(600) || GetStageDone(700)
		Return
	EndIf

	If BoS03_TryStartEN01()
		SetStage(700)
	Else
		StartTimer(5.0, 600)
	EndIf
EndEvent
