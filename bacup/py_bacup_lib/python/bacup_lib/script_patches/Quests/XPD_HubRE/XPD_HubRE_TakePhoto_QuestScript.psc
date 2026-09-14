Function PreparePhotoEncounter()
	SelectSceneLocation()
	SelectPhotoTarget()
EndFunction

Function SelectPhotoTarget()
	If ChosenTarget != None && Alias_PhotoTarget.GetReference() != None
		Return
	EndIf
	If Targets == None || Targets.Length == 0
		Return
	EndIf

	Int startIndex = Utility.RandomInt(0, Targets.Length - 1)
	Int offset = 0
	While offset < Targets.Length
		PossiblePhotoTarget candidate = Targets[(startIndex + offset) % Targets.Length]
		If candidate != None && candidate.ThingToPhotograph != None && candidate.ThingToPhotograph.GetReference() != None
			ChosenTarget = candidate
			Alias_PhotoTarget.ForceRefTo(candidate.ThingToPhotograph.GetReference())
			photoAlias = candidate.Alias_PhotoItem
			photographItem = candidate.itemPhotoOfTarget
			If candidate.StageToSet > 0 && !IsStageDone(candidate.StageToSet)
				SetStage(candidate.StageToSet)
			EndIf
			Return
		EndIf
		offset += 1
	EndWhile
EndFunction

Function BeginLocalPhotoValidation(MiscObject cameraItem)
	If cameraItem != None
		P01C_Bucket_BrokenCamera = cameraItem
	EndIf
	PlayerRef = Alias_Player.GetActorReference()
	CancelTimer(1)
	StartTimer(1.0, 1)
EndFunction

Int Function GetChosenPhotoStage()
	If ChosenTarget == None
		Return 0
	EndIf
	Return ChosenTarget.StageToSet
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != 1
		Parent.OnTimer(aiTimerID)
		Return
	EndIf
	If !IsRunning() || IsStageDone(CompletionStage)
		Return
	EndIf
	If PlayerRef == None
		PlayerRef = Alias_Player.GetActorReference()
	EndIf
	ObjectReference targetRef = Alias_PhotoTarget.GetReference()
	If PlayerRef != None && targetRef != None && P01C_Bucket_BrokenCamera != None && PlayerRef.GetItemCount(P01C_Bucket_BrokenCamera) > 0 && PlayerRef.GetDistance(targetRef) <= 300.0
		If photographItem != None && PlayerRef.GetItemCount(photographItem) < 1
			PlayerRef.AddItem(photographItem, 1, True)
		EndIf
		SetStage(CompletionStage)
		Return
	EndIf
	StartTimer(1.0, 1)
EndEvent

Function ClearPhotoSelection()
	CancelTimer(1)
	Alias_PhotoTarget.Clear()
	ChosenTarget = None
	photoAlias = None
	photographItem = None
	PlayerRef = None
	ClearLocalSelection()
EndFunction

Event OnQuestShutdown()
	ClearPhotoSelection()
	Parent.OnQuestShutdown()
EndEvent
