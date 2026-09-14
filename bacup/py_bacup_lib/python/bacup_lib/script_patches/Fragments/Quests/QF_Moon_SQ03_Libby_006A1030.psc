Function Fragment_Stage_0100_Item_00()
	Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
	RegisterForQuestReferences()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0325_Item_00()
	SetObjectiveCompleted(25)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0350_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(35)
	If !IsStageDone(360)
		SetStage(360)
	EndIf
EndFunction

Function Fragment_Stage_0375_Item_00()
	SetObjectiveCompleted(35)
	SetObjectiveDisplayed(36)
	If !IsStageDone(380)
		SetStage(380)
	EndIf
EndFunction

Function Fragment_Stage_0380_Item_00()
	If Alias_enemy_Wendigo != None && Alias_enemy_Wendigo.GetReference() != None
		Alias_enemy_Wendigo.GetReference().Enable()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(36)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	If !IsStageDone(510)
		SetStage(510)
	EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
	SetObjectiveDisplayed(50)
	If !IsStageDone(600)
		SetStage(600)
	EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
	Fragment_Stage_0510_Item_00()
EndFunction

Function Fragment_Stage_0530_Item_00()
	Fragment_Stage_0510_Item_00()
EndFunction

Function Fragment_Stage_0600_Item_00()
EndFunction

Function Fragment_Stage_0610_Item_00()
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
	Fragment_Stage_0610_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(50)
	Stop()
EndFunction

Function RegisterForQuestReferences()
	RegisterForAliasActivation(0)
	RegisterForAliasActivation(1)
	RegisterForAliasActivation(6)
	RegisterForAliasActivation(16)
EndFunction

Function RegisterForAliasActivation(Int aiAliasID)
	ReferenceAlias targetAlias = GetAlias(aiAliasID) as ReferenceAlias
	If targetAlias != None && targetAlias.GetReference() != None
		RegisterForRemoteEvent(targetAlias.GetReference(), "OnActivate")
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	Actor player = Game.GetPlayer()
	If player == None || akActivator != player
		Return
	EndIf

	ReferenceAlias libbyAlias = GetAlias(0) as ReferenceAlias
	ReferenceAlias vinnyAlias = GetAlias(1) as ReferenceAlias
	ReferenceAlias partsActivator = GetAlias(6) as ReferenceAlias
	ReferenceAlias shovelActivator = GetAlias(16) as ReferenceAlias

	If shovelActivator != None && akSender == shovelActivator.GetReference() && IsStageDone(350) && !IsStageDone(375)
		Form shovel = Game.GetFormFromFile(0x006B5BE3, "SeventySix.esm")
		If shovel != None
			player.AddItem(shovel, 1, True)
			SetStage(375)
		EndIf
		Return
	ElseIf partsActivator != None && akSender == partsActivator.GetReference() && IsStageDone(375) && !IsStageDone(400)
		If MiscObject_RepairParts != None
			player.AddItem(MiscObject_RepairParts, 1, True)
			SetStage(400)
		EndIf
		Return
	EndIf

	Int nextStage = -1
	If vinnyAlias != None && akSender == vinnyAlias.GetReference() && IsStageDone(600) && !IsStageDone(610)
		nextStage = 610
	ElseIf libbyAlias != None && akSender == libbyAlias.GetReference() && IsStageDone(400) && !IsStageDone(500)
		nextStage = 500
	ElseIf libbyAlias != None && akSender == libbyAlias.GetReference() && IsStageDone(200) && !IsStageDone(300)
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
