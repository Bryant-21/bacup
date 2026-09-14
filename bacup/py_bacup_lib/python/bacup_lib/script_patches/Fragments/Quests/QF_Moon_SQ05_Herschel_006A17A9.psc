Function Fragment_Stage_0100_Item_00()
	If Alias_Player != None
		Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
	EndIf
	RegisterForQuestActors()
	SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200)
	SetPageCount()
	SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0410_Item_00()
	UpdatePageProgress()
EndFunction

Function Fragment_Stage_0420_Item_00()
	UpdatePageProgress()
EndFunction

Function Fragment_Stage_0430_Item_00()
	UpdatePageProgress()
EndFunction

Function Fragment_Stage_0440_Item_00()
	UpdatePageProgress()
EndFunction

Function Fragment_Stage_0450_Item_00()
	UpdatePageProgress()
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetPageCount()
	SetObjectiveCompleted(300)
	SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0600_Item_00()
	RemovePages()
	SetObjectiveCompleted(400)
	If !IsStageDone(610)
		SetStage(610)
	EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
	If !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
	Fragment_Stage_0610_Item_00()
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(500)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	RemovePages()
	Stop()
EndFunction

Function UpdatePageProgress()
	SetPageCount()
	If IsStageDone(410) && IsStageDone(420) && IsStageDone(430) && IsStageDone(440) && IsStageDone(450) && !IsStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function SetPageCount()
	Int count = 0
	If IsStageDone(410)
		count += 1
	EndIf
	If IsStageDone(420)
		count += 1
	EndIf
	If IsStageDone(430)
		count += 1
	EndIf
	If IsStageDone(440)
		count += 1
	EndIf
	If IsStageDone(450)
		count += 1
	EndIf
	Actor player = Game.GetPlayer()
	If player != None && Moon_SQ05_AV_PagesCurrent != None
		player.SetValue(Moon_SQ05_AV_PagesCurrent, count)
	EndIf
EndFunction

Function RemovePages()
	Actor player = Game.GetPlayer()
	If player == None
		Return
	EndIf
	player.RemoveItem(Moon_SQ05_Page1, 1, True)
	player.RemoveItem(Moon_SQ05_Page2, 1, True)
	player.RemoveItem(Moon_SQ05_Page3, 1, True)
	player.RemoveItem(Moon_SQ05_Page4, 1, True)
	player.RemoveItem(Moon_SQ05_Page5, 1, True)
EndFunction

Function RegisterForQuestActors()
	RegisterForActorAlias(3)
	RegisterForActorAlias(5)
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
	ReferenceAlias vinnyAlias = GetAlias(3) as ReferenceAlias
	ReferenceAlias herschelAlias = GetAlias(5) as ReferenceAlias
	Int nextStage = -1
	If vinnyAlias != None && akSender == vinnyAlias.GetReference() && IsStageDone(700) && !IsStageDone(800)
		nextStage = 800
	ElseIf herschelAlias != None && akSender == herschelAlias.GetReference() && IsStageDone(500) && !IsStageDone(600)
		nextStage = 600
	ElseIf herschelAlias != None && akSender == herschelAlias.GetReference() && IsStageDone(200) && !IsStageDone(300)
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
