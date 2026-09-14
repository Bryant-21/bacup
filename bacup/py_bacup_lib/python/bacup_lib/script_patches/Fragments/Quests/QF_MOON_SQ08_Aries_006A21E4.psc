Function Fragment_Stage_0100_Item_00()
	Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
	RegisterForQuestActors()
	SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100)
	PlaceHolotapes()
	SetObjectiveDisplayed(210)
	SetObjectiveDisplayed(220)
	SetObjectiveDisplayed(230)
	SetObjectiveDisplayed(240)
	SetObjectiveDisplayed(250)
EndFunction

Function Fragment_Stage_0410_Item_00()
	SetObjectiveCompleted(210)
	CheckHolotapeProgress()
EndFunction

Function Fragment_Stage_0420_Item_00()
	SetObjectiveCompleted(220)
	CheckHolotapeProgress()
EndFunction

Function Fragment_Stage_0430_Item_00()
	SetObjectiveCompleted(230)
	CheckHolotapeProgress()
EndFunction

Function Fragment_Stage_0440_Item_00()
	SetObjectiveCompleted(240)
	CheckHolotapeProgress()
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveCompleted(250)
	CheckHolotapeProgress()
EndFunction

Function Fragment_Stage_0550_Item_00()
	Actor player = Game.GetPlayer()
	If player != None && MOON_SQ08_Aries_Holotape_Scene06_Tape06 != None && player.GetItemCount(MOON_SQ08_Aries_Holotape_Scene06_Tape06) == 0
		player.AddItem(MOON_SQ08_Aries_Holotape_Scene06_Tape06, 1, True)
	EndIf
	SetObjectiveCompleted(200)
	SetObjectiveDisplayed(300)
	SetObjectiveDisplayed(325)
	SetObjectiveDisplayed(350)
EndFunction

Function Fragment_Stage_0600_Item_00()
	Actor player = Game.GetPlayer()
	If player != None && MOON_SQ08_Aries_AV_BurnedTapes != None
		player.SetValue(MOON_SQ08_Aries_AV_BurnedTapes, 1.0)
	EndIf
	RemoveHolotapes()
	SetObjectiveCompleted(300)
	SetObjectiveCompleted(325)
	SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0650_Item_00()
	SetObjectiveCompleted(400)
	Stop()
EndFunction

Function Fragment_Stage_0700_Item_00()
	RemoveHolotapes()
	SetObjectiveCompleted(300)
	SetObjectiveCompleted(325)
	SetObjectiveDisplayed(500)
	RegisterForActorAlias(57)
	RegisterForActorAlias(58)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(500)
EndFunction

Function Fragment_Stage_0850_Item_00()
	If MOON_SQ08_Aries_PostQuestScene != None
		MOON_SQ08_Aries_PostQuestScene.Start()
	EndIf
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
EndFunction

Function Fragment_Stage_0950_Item_00()
	SetObjectiveCompleted(350)
EndFunction

Function Fragment_Stage_9999_Item_00()
	RemoveHolotapes()
	Stop()
EndFunction

Function PlaceHolotapes()
	PlaceHolotape(Alias_container_Holo_ScootsCabin_05, MOON_SQ08_Aries_Holotape_Scene01_Tape05)
	PlaceHolotape(Alias_container_Holo_LostHome_04, MOON_SQ08_Aries_Holotape_Scene02_Tape04)
	PlaceHolotape(Alias_container_Holo_Welch_03, MOON_SQ08_Aries_Holotape_Scene03_Tape03)
	PlaceHolotape(Alias_container_Holo_Uncanny_02, MOON_SQ08_Aries_Holotape_Scene04_Tape02)
	PlaceHolotape(Alias_container_Holo_VanLowe_01, MOON_SQ08_Aries_Holotape_Scene05_Tape01)
EndFunction

Function PlaceHolotape(ReferenceAlias akContainerAlias, Form akHolotape)
	Actor player = Game.GetPlayer()
	ObjectReference container = None
	If akContainerAlias != None
		container = akContainerAlias.GetReference()
	EndIf
	If container != None && akHolotape != None && container.GetItemCount(akHolotape) == 0 && (player == None || player.GetItemCount(akHolotape) == 0)
		container.AddItem(akHolotape, 1, True)
	EndIf
EndFunction

Function CheckHolotapeProgress()
	If IsStageDone(410) && IsStageDone(420) && IsStageDone(430) && IsStageDone(440) && IsStageDone(450) && !IsStageDone(550)
		SetObjectiveDisplayed(200)
	EndIf
EndFunction

Function RemoveHolotapes()
	Actor player = Game.GetPlayer()
	If player == None
		Return
	EndIf
	player.RemoveItem(MOON_SQ08_Aries_Holotape_Scene01_Tape05, 1, True)
	player.RemoveItem(MOON_SQ08_Aries_Holotape_Scene02_Tape04, 1, True)
	player.RemoveItem(MOON_SQ08_Aries_Holotape_Scene03_Tape03, 1, True)
	player.RemoveItem(MOON_SQ08_Aries_Holotape_Scene04_Tape02, 1, True)
	player.RemoveItem(MOON_SQ08_Aries_Holotape_Scene05_Tape01, 1, True)
	player.RemoveItem(MOON_SQ08_Aries_Holotape_Scene06_Tape06, 1, True)
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
	ReferenceAlias ariesAlias = GetAlias(39) as ReferenceAlias
	ReferenceAlias interiorAriesAlias = GetAlias(57) as ReferenceAlias
	ReferenceAlias interiorCarverAlias = GetAlias(58) as ReferenceAlias
	Int nextStage = -1
	If interiorCarverAlias != None && akSender == interiorCarverAlias.GetReference() && IsStageDone(700) && !IsStageDone(800)
		nextStage = 800
	ElseIf interiorAriesAlias != None && akSender == interiorAriesAlias.GetReference() && IsStageDone(800) && !IsStageDone(850)
		nextStage = 850
	ElseIf ariesAlias != None && akSender == ariesAlias.GetReference() && IsStageDone(600) && !IsStageDone(650)
		nextStage = 650
	ElseIf carverAlias != None && akSender == carverAlias.GetReference() && IsStageDone(550) && !IsStageDone(600) && !IsStageDone(700)
		nextStage = 700
	ElseIf ariesAlias != None && akSender == ariesAlias.GetReference() && IsStageDone(450) && !IsStageDone(550)
		nextStage = 550
	ElseIf ariesAlias != None && akSender == ariesAlias.GetReference() && IsStageDone(100) && !IsStageDone(200)
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
