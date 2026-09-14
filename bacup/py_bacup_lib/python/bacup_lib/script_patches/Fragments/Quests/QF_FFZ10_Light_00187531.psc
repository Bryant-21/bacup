Function SetEventReferenceEnabled(ObjectReference akReference, Bool abEnabled)
	If akReference == None
		Return
	EndIf
	If abEnabled
		akReference.Enable(False)
	Else
		akReference.Disable(False)
	EndIf
EndFunction

Function ResetEventObjectives()
	SetObjectiveDisplayed(1, False)
	SetObjectiveCompleted(1, False)
	SetObjectiveFailed(1, False)
	SetObjectiveDisplayed(5, False)
	SetObjectiveCompleted(5, False)
	SetObjectiveFailed(5, False)
	SetObjectiveDisplayed(10, False)
	SetObjectiveCompleted(10, False)
	SetObjectiveFailed(10, False)
	SetObjectiveDisplayed(20, False)
	SetObjectiveCompleted(20, False)
	SetObjectiveFailed(20, False)
EndFunction

Function AddPlayerToCommuneCollection()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	RefCollectionAlias communeCollection = Alias_PlayersCanCommune
	If communeCollection == None
		communeCollection = Alias_Players_CanCommune
	EndIf
	If communeCollection != None && communeCollection.Find(playerRef) < 0
		communeCollection.AddRef(playerRef)
	EndIf
EndFunction

Function RemovePlayerFromCommuneCollection()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	If Alias_PlayersCanCommune != None && Alias_PlayersCanCommune.Find(playerRef) >= 0
		Alias_PlayersCanCommune.RemoveRef(playerRef)
	EndIf
	If Alias_Players_CanCommune != None && Alias_Players_CanCommune.Find(playerRef) >= 0
		Alias_Players_CanCommune.RemoveRef(playerRef)
	EndIf
EndFunction

Function SetEventWorldEnabled(Bool abEnabled)
	SetEventReferenceEnabled(FFZ10_Light_EnableMarker, abEnabled)
	SetEventReferenceEnabled(FFZ10_Light_BeaconRef, abEnabled)
	SetEventReferenceEnabled(FFZ10_Light_LampEnableMarkerRef, abEnabled)
EndFunction

Function ShutdownEvent(Bool abFailed)
	FFZ10_Light_QuestScript eventScript = (Self as Quest) as FFZ10_Light_QuestScript
	If eventScript != None
		eventScript.CancelShutdownTimer()
	EndIf

	RemovePlayerFromCommuneCollection()
	SetEventWorldEnabled(False)
	If Alias_SpawnArea1 != None
		SetEventReferenceEnabled(Alias_SpawnArea1.GetReference(), False)
	EndIf
	If FFZ10_Light_Global != None
		FFZ10_Light_Global.SetValue(0.0)
	EndIf

	If abFailed
		If IsObjectiveDisplayed(1) && !IsObjectiveCompleted(1)
			SetObjectiveFailed(1, True)
		EndIf
		If IsObjectiveDisplayed(5) && !IsObjectiveCompleted(5)
			SetObjectiveFailed(5, True)
		EndIf
		If IsObjectiveDisplayed(10) && !IsObjectiveCompleted(10)
			SetObjectiveFailed(10, True)
		EndIf
		If IsObjectiveDisplayed(20) && !IsObjectiveCompleted(20)
			SetObjectiveFailed(20, True)
		EndIf
	EndIf
	Stop()
EndFunction

Function Fragment_Stage_0005_Item_00()
	If FFZ10_Light_Global != None
		FFZ10_Light_Global.SetValue(0.0)
	EndIf
	ResetEventObjectives()
	RemovePlayerFromCommuneCollection()
	SetEventWorldEnabled(False)
	If Alias_SpawnArea1 != None
		SetEventReferenceEnabled(Alias_SpawnArea1.GetReference(), False)
	EndIf
	SetObjectiveDisplayed(1, True, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
	SetObjectiveCompleted(1, True)
	If !IsStageDone(20)
		SetStage(20)
	EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
	If Alias_SpawnArea1 != None
		SetEventReferenceEnabled(Alias_SpawnArea1.GetReference(), True)
	EndIf
	If !IsStageDone(30)
		SetStage(30)
	EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
	SetObjectiveDisplayed(5, True, True)
	SetObjectiveDisplayed(10, True, True)
	If Alias_SpawnArea1 != None
		SetEventReferenceEnabled(Alias_SpawnArea1.GetReference(), True)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	If !IsObjectiveCompleted(5)
		SetObjectiveDisplayed(5, True)
	EndIf
	If !IsObjectiveCompleted(10)
		SetObjectiveDisplayed(10, True)
	EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
	If !IsStageDone(1500)
		SetStage(1500)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(5, True)
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(20, True, True)
	If FFZ10_Light_Global != None
		FFZ10_Light_Global.SetValue(30.0)
	EndIf

	SetEventWorldEnabled(True)
	AddPlayerToCommuneCollection()

	Actor mothmanRef = None
	If Alias_WiseMothman != None
		mothmanRef = Alias_WiseMothman.GetActorReference()
	EndIf
	If mothmanRef != None
		mothmanRef.Enable(False)
		If FFZ10_Light_WiseMothmanFaction != None
			mothmanRef.AddToFaction(FFZ10_Light_WiseMothmanFaction)
		EndIf
		If AmbushRelease != None
			mothmanRef.SetValue(AmbushRelease, 1.0)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20, True)
	FFZ10_Light_QuestScript eventScript = (Self as Quest) as FFZ10_Light_QuestScript
	If eventScript != None
		eventScript.StartShutdownTimer()
	EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
	ShutdownEvent(True)
EndFunction

Function Fragment_Stage_1600_Item_00()
	ShutdownEvent(False)
EndFunction

Function Fragment_Stage_9991_Item_00()
	ShutdownEvent(True)
EndFunction
