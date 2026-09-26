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

Function StartWave(String asWaveID)
	DefaultQuestEncounterWaveScript waveScript = (Self as Quest) as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StartEncounterWaveByID(asWaveID)
	EndIf
EndFunction

Function UpdateInvestigateCount()
	B21:QuestVariables variables = (Self as Quest) as B21:QuestVariables
	If variables == None
		Return
	EndIf
	Int found = 0
	If IsStageDone(610)
		found += 1
	EndIf
	If IsStageDone(625)
		found += 1
	EndIf
	If IsStageDone(630)
		found += 1
	EndIf
	variables.SetVariable("InvestigateMax", 3.0)
	variables.SetVariable("InvestigateCurrent", found as Float)
EndFunction

Function EnableInvestigation(ReferenceAlias akInvestigation)
	If akInvestigation != None
		ObjectReference investigationRef = akInvestigation.GetReference()
		If investigationRef != None && investigationRef.IsDisabled()
			investigationRef.Enable()
		EndIf
	EndIf
EndFunction

; FO76 lets the player skip Craig and the survivors and still finish on reaching the manor, but the manor alias only arms at 700.
Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	If IsStageDone(400) && !IsStageDone(800)
		SetStage(800)
	EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(15)
	If !IsStageDone(160)
		SetStage(160)
	EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
	StartWave("V63E Roamers - Lost")
EndFunction

Function Fragment_Stage_0170_Item_00()
	SetObjectiveCompleted(15)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0200_Item_00()
	StartWave("V63E Wave 1 - Lost Mixed")
EndFunction

Function Fragment_Stage_0250_Item_00()
	StartWave("V63E Wave 2 - Lost Mixed")
EndFunction

Function Fragment_Stage_0260_Item_00()
	StartWave("V63E Wave 3 - Lost Mixed")
EndFunction

Function Fragment_Stage_0290_Item_00()
	SetObjectiveCompleted(15)
	SetObjectiveCompleted(20)
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
	SetObjectiveDisplayed(40)
	ReferenceAlias manorMarker = GetAlias(22) as ReferenceAlias
	If Alias_Player != None && manorMarker != None
		RegisterForDistanceLessThanEvent(Alias_Player, manorMarker, 9000.0)
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
	SetObjectiveCompleted(40)
	SetObjectiveCompleted(45)
	SetObjectiveCompleted(50)
	UpdateInvestigateCount()
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
	UpdateInvestigateCount()
	EnableInvestigation(Alias_Investigation02)
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If Storm_MISC_CraigItems_StartKeyword != None
		Storm_MISC_CraigItems_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
	StartWave("MRT Ambush - Ferals & Gunner")
	If !IsStageDone(625)
		SetStage(625)
	EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
	PlaceFlareAt(Alias_Flare02)
	UpdateInvestigateCount()
	EnableInvestigation(Alias_Investigation03)
EndFunction

Function Fragment_Stage_0630_Item_00()
	UpdateInvestigateCount()
	StartWave("DHMG Ambush - Ferals & Gunner")
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
