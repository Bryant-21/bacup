Function Fragment_Stage_0100_Item_00()
	If Alias_Player != None
		Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
	EndIf
	RegisterForQuestActors()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
	If !IsStageDone(425)
		SetStage(425)
	EndIf
EndFunction

Function Fragment_Stage_0425_Item_00()
	Actor player = Game.GetPlayer()
	If player != None
		player.RemoveItem(Base_Envelope, 1, True)
		player.AddItem(Base_Letter, 1, True)
	EndIf
	If !IsStageDone(475)
		SetStage(475)
	EndIf
EndFunction

Function Fragment_Stage_0475_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
	Actor player = Game.GetPlayer()
	If player != None
		player.RemoveItem(Base_Letter, 1, True)
	EndIf
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
	Stop()
EndFunction

Function RegisterForQuestActors()
	RegisterForActorAlias(0)
	RegisterForActorAlias(1)
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

	ReferenceAlias eugenieAlias = GetAlias(0) as ReferenceAlias
	ReferenceAlias vinnyAlias = GetAlias(1) as ReferenceAlias
	Int nextStage = -1
	If vinnyAlias != None && akSender == vinnyAlias.GetReference() && IsStageDone(500) && !IsStageDone(600)
		nextStage = 600
	ElseIf eugenieAlias != None && akSender == eugenieAlias.GetReference() && IsStageDone(475) && !IsStageDone(500)
		nextStage = 500
	ElseIf eugenieAlias != None && akSender == eugenieAlias.GetReference() && IsStageDone(200) && !IsStageDone(300)
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
