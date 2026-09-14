Function PlaceFlareAt(ReferenceAlias flareMarker)
	If flareMarker == None
		Return
	EndIf
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && Misc_Flares != None && playerRef.GetItemCount(Misc_Flares) > 0
		playerRef.RemoveItem(Misc_Flares, 1, True)
	EndIf
	ObjectReference markerRef = flareMarker.GetReference()
	If markerRef != None && MStatic_Flare != None
		ObjectReference flareRef = markerRef.PlaceAtMe(MStatic_Flare)
		If flareRef != None && RefCol_Flares != None
			RefCol_Flares.AddRef(flareRef)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(15)
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0170_Item_00()
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0260_Item_00()
	If !IsStageDone(290)
		SetStage(290)
	EndIf
EndFunction

Function Fragment_Stage_0290_Item_00()
	If Scene_Hilda_Callout != None && !Scene_Hilda_Callout.IsPlaying()
		Scene_Hilda_Callout.Start()
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(15)
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	If !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
	If Scene_Craig_Callouts != None && !Scene_Craig_Callouts.IsPlaying()
		Scene_Craig_Callouts.Start()
	EndIf
	If !IsStageDone(450)
		SetStage(450)
	EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(45)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0550_Item_00()
	SetObjectiveCompleted(45)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && Misc_Flares != None
		Int flareCount = playerRef.GetItemCount(Misc_Flares)
		If flareCount < 3
			playerRef.AddItem(Misc_Flares, 3 - flareCount, False)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
	PlaceFlareAt(Alias_Flare01)
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If Storm_MISC_CraigItems_StartKeyword != None
		Storm_MISC_CraigItems_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
	If !IsStageDone(625)
		SetStage(625)
	EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
	PlaceFlareAt(Alias_Flare02)
EndFunction

Function Fragment_Stage_0630_Item_00()
	If !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	If IsStageDone(630)
		PlaceFlareAt(Alias_Flare03)
	EndIf
	SetObjectiveCompleted(40)
	SetObjectiveCompleted(45)
	SetObjectiveCompleted(50)
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(70)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	Storm_MQ03_IntroPt2_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	If !IsStageDone(9500)
		SetStage(9500)
	EndIf
EndFunction

Function Fragment_Stage_9500_Item_00()
	Stop()
EndFunction
