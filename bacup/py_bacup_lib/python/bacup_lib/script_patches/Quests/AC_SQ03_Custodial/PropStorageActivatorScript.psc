Event OnAliasInit()
	OwningQuest = GetOwningQuest()
EndEvent

Event OnActivate(ObjectReference akActionRef)
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	If OwningQuest == None || akActionRef != Game.GetPlayer() || PropData == None
		Return
	EndIf

	Int index = 0
	While index < PropData.Length
		PropDatum currentProp = PropData[index]
		If currentProp != None && currentProp.FoundStage > 0 && currentProp.PlacedStage > 0 && OwningQuest.IsStageDone(currentProp.FoundStage) && !OwningQuest.IsStageDone(currentProp.PlacedStage)
			If currentProp.PropToEnable != None && currentProp.PropToEnable.GetReference() != None
				currentProp.PropToEnable.GetReference().Enable()
			EndIf
			OwningQuest.SetStage(currentProp.PlacedStage)
		EndIf
		index += 1
	EndWhile
EndEvent
