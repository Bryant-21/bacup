Function Fragment_Stage_0100_Item_00()
	Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
	RegisterForQuestActors()
	SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0210_Item_00()
EndFunction

Function Fragment_Stage_0220_Item_00()
EndFunction

Function Fragment_Stage_0230_Item_00()
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200)
	Actor player = Game.GetPlayer()
	If player != None && MOON_SQ04_Rudy_ShoppingList != None && player.GetItemCount(MOON_SQ04_Rudy_ShoppingList) == 0
		player.AddItem(MOON_SQ04_Rudy_ShoppingList, 1, True)
	EndIf
	SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0410_Item_00()
	CheckIngredientProgress()
EndFunction

Function Fragment_Stage_0420_Item_00()
	CheckIngredientProgress()
EndFunction

Function Fragment_Stage_0430_Item_00()
	CheckIngredientProgress()
EndFunction

Function Fragment_Stage_0440_Item_00()
	CheckIngredientProgress()
EndFunction

Function Fragment_Stage_0450_Item_00()
	CheckIngredientProgress()
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(300)
	SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(400)
	If !IsStageDone(610)
		SetStage(610)
	EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
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

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(500)
	Actor player = Game.GetPlayer()
	If player != None
		player.RemoveItem(MOON_SQ04_Rudy_ShoppingList, 1, True)
	EndIf
	Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
	Stop()
EndFunction

Function CheckIngredientProgress()
	If IsStageDone(410) && IsStageDone(420) && IsStageDone(430) && IsStageDone(440) && IsStageDone(450) && !IsStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function RegisterForQuestActors()
	RegisterForActorAlias(1)
	RegisterForActorAlias(2)
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

	ReferenceAlias vinnyAlias = GetAlias(1) as ReferenceAlias
	ReferenceAlias rudyAlias = GetAlias(2) as ReferenceAlias
	Int nextStage = -1
	If vinnyAlias != None && akSender == vinnyAlias.GetReference() && IsStageDone(700) && !IsStageDone(9000)
		nextStage = 9000
	ElseIf rudyAlias != None && akSender == rudyAlias.GetReference() && IsStageDone(500) && !IsStageDone(600)
		nextStage = 600
	ElseIf rudyAlias != None && akSender == rudyAlias.GetReference() && IsStageDone(200) && !IsStageDone(300)
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
