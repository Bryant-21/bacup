Function Fragment_Stage_0100_Item_00()
	Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
	RegisterForQuestActors()
	SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200)
	SetObjectiveDisplayed(300)
	SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300)
	SetObjectiveDisplayed(350)
	CheckSearchProgress()
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveCompleted(350)
	SetObjectiveDisplayed(400)
	CheckSearchProgress()
EndFunction

Function Fragment_Stage_0475_Item_00()
	SetObjectiveCompleted(1000)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(400)
EndFunction

Function Fragment_Stage_0610_Item_00()
	Actor player = Game.GetPlayer()
	If player != None
		player.RemoveItem(MOON_SQ07_Carver_Misc_Lighter, 1, True)
		If MOON_SQ07_Carver_AV_GaveWatch != None
			player.SetValue(MOON_SQ07_Carver_AV_GaveWatch, 1.0)
		EndIf
	EndIf
	If !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
	If !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(500)
	Stop()
EndFunction

Function CheckSearchProgress()
	If IsStageDone(400) && IsStageDone(450) && !IsStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function RegisterForQuestActors()
	RegisterForActorAlias(38)
	RegisterForActorAlias(39)
EndFunction

Function RegisterForActorAlias(Int aiAliasID)
	ReferenceAlias actorAlias = GetAlias(aiAliasID) as ReferenceAlias
	If actorAlias != None && actorAlias.GetReference() != None
		RegisterForRemoteEvent(actorAlias.GetReference(), "OnActivate")
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	Actor player = Game.GetPlayer()
	If player == None || akActivator != player
		Return
	EndIf
	ReferenceAlias carverAlias = GetAlias(38) as ReferenceAlias
	ReferenceAlias vinnyAlias = GetAlias(39) as ReferenceAlias
	Int nextStage = -1
	If vinnyAlias != None && akSender == vinnyAlias.GetReference() && IsStageDone(700) && !IsStageDone(9000)
		nextStage = 9000
	ElseIf carverAlias != None && akSender == carverAlias.GetReference() && IsStageDone(500) && !IsStageDone(610) && !IsStageDone(620)
		If player.GetItemCount(MOON_SQ07_Carver_Misc_Lighter) > 0
			nextStage = 610
		Else
			nextStage = 620
		EndIf
	ElseIf carverAlias != None && akSender == carverAlias.GetReference() && IsStageDone(200) && !IsStageDone(300)
		nextStage = 300
	ElseIf vinnyAlias != None && akSender == vinnyAlias.GetReference() && IsStageDone(100) && !IsStageDone(200)
		nextStage = 200
	EndIf
	WaitForDialogueAndSetStage(akSender as Actor, player, nextStage)
EndEvent

Function WaitForDialogueAndSetStage(Actor akSpeaker, Actor akPlayer, Int aiStage)
	If akSpeaker == None || akPlayer == None || aiStage < 0
		Return
	EndIf
	Int checks = 0
	While akSpeaker.GetDialogueTarget() != akPlayer && checks < 40
		Utility.Wait(0.25)
		checks += 1
	EndWhile
	If akSpeaker.GetDialogueTarget() != akPlayer
		Return
	EndIf
	checks = 0
	While akSpeaker.GetDialogueTarget() == akPlayer && checks < 2400
		Utility.Wait(0.25)
		checks += 1
	EndWhile
	If akSpeaker.GetDialogueTarget() != akPlayer && !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction
