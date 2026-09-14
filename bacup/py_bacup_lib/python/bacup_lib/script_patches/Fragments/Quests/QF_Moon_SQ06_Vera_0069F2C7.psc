Function Fragment_Stage_0100_Item_00()
	If Alias_Player != None
		Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
	EndIf
	RegisterForQuestReferences()
	SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0205_Item_00()
EndFunction

Function Fragment_Stage_0210_Item_00()
	Actor player = Game.GetPlayer()
	Form camera = Game.GetFormFromFile(0x0046F481, "SeventySix.esm")
	If player != None && camera != None && player.GetItemCount(camera) > 0
		SetStage(300)
	ElseIf player != None && MiscObj_BrokenCamera != None && player.GetItemCount(MiscObj_BrokenCamera) > 0
		SetStage(260)
	Else
		SetStage(250)
	EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
	If !IsStageDone(210)
		SetStage(210)
	EndIf
EndFunction

Function Fragment_Stage_0240_Item_01()
	Fragment_Stage_0240_Item_00()
EndFunction

Function Fragment_Stage_0240_Item_02()
	Fragment_Stage_0240_Item_00()
EndFunction

Function Fragment_Stage_0250_Item_00()
	SetObjectiveCompleted(200)
	SetObjectiveDisplayed(250)
EndFunction

Function Fragment_Stage_0255_Item_00()
EndFunction

Function Fragment_Stage_0260_Item_00()
	Actor player = Game.GetPlayer()
	Form camera = Game.GetFormFromFile(0x0046F481, "SeventySix.esm")
	If player != None && camera != None
		player.RemoveItem(MiscObj_BrokenCamera, 1, True)
		player.AddItem(camera, 1, True)
		If !IsStageDone(265)
			SetStage(265)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0265_Item_00()
	SetObjectiveCompleted(250)
	SetObjectiveCompleted(260)
	If !IsStageDone(300)
		SetStage(300)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200)
	SetObjectiveDisplayed(300)
	SetObjectiveDisplayed(310)
	SetObjectiveDisplayed(320)
	SetObjectiveDisplayed(330)
EndFunction

Function Fragment_Stage_0405_Item_00()
	If !IsStageDone(410)
		SetStage(410)
	EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
	SetObjectiveCompleted(310)
	CheckPhotoProgress()
EndFunction

Function Fragment_Stage_0415_Item_00()
	If !IsStageDone(420)
		SetStage(420)
	EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
	SetObjectiveCompleted(320)
	CheckPhotoProgress()
EndFunction

Function Fragment_Stage_0425_Item_00()
	If !IsStageDone(430)
		SetStage(430)
	EndIf
EndFunction

Function Fragment_Stage_0430_Item_00()
	SetObjectiveCompleted(330)
	CheckPhotoProgress()
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(300)
	SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0550_Item_00()
	If !IsStageDone(610)
		SetStage(610)
	EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
	SetObjectiveCompleted(400)
	SetObjectiveDisplayed(500)
	If !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
	Fragment_Stage_0610_Item_00()
EndFunction

Function Fragment_Stage_0630_Item_00()
	Fragment_Stage_0610_Item_00()
EndFunction

Function Fragment_Stage_0640_Item_00()
	Fragment_Stage_0610_Item_00()
EndFunction

Function Fragment_Stage_0650_Item_00()
	Fragment_Stage_0610_Item_00()
EndFunction

Function Fragment_Stage_0700_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(500)
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Stop()
EndFunction

Function CheckPhotoProgress()
	If IsStageDone(410) && IsStageDone(420) && IsStageDone(430) && !IsStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function RegisterForQuestReferences()
	RegisterForAliasActivation(3)
	RegisterForAliasActivation(4)
	RegisterForAliasActivation(19)
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
	ReferenceAlias vinnyAlias = GetAlias(3) as ReferenceAlias
	ReferenceAlias veraAlias = GetAlias(4) as ReferenceAlias
	ReferenceAlias bagAlias = GetAlias(19) as ReferenceAlias
	If bagAlias != None && akSender == bagAlias.GetReference() && IsStageDone(250) && !IsStageDone(260)
		If MiscObj_BrokenCamera != None && player.GetItemCount(MiscObj_BrokenCamera) == 0
			player.AddItem(MiscObj_BrokenCamera, 1, True)
		EndIf
		SetStage(255)
		SetStage(260)
		Return
	EndIf

	Int nextStage = -1
	If vinnyAlias != None && akSender == vinnyAlias.GetReference() && IsStageDone(700) && !IsStageDone(9000)
		nextStage = 9000
	ElseIf veraAlias != None && akSender == veraAlias.GetReference() && IsStageDone(500) && !IsStageDone(550)
		nextStage = 550
	ElseIf veraAlias != None && akSender == veraAlias.GetReference() && IsStageDone(200) && !IsStageDone(210)
		nextStage = 210
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
