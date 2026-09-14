Function PrepareRepairEncounter()
	SelectSceneLocation()
	SelectRobotLocation()
	SelectRepairItem()
EndFunction

Function SelectRobotLocation()
	If ChosenArea != None && ChosenRobotLocation.GetReference() != None
		Return
	EndIf
	If RobotLocs == None || RobotLocs.Length == 0
		Return
	EndIf

	Int startIndex = Utility.RandomInt(0, RobotLocs.Length - 1)
	Int offset = 0
	While offset < RobotLocs.Length
		Int index = (startIndex + offset) % RobotLocs.Length
		RobotLocation candidate = RobotLocs[index]
		If candidate != None && candidate.AreaLocations != None && candidate.AreaLocations.GetCount() > 0
			ObjectReference marker = candidate.AreaLocations.GetAt(Utility.RandomInt(0, candidate.AreaLocations.GetCount() - 1))
			If marker != None
				ChosenArea = candidate
				ChosenRobotLocation.ForceRefTo(marker)
				If candidate.StageToSet > 0 && !IsStageDone(candidate.StageToSet)
					SetStage(candidate.StageToSet)
				EndIf
				Return
			EndIf
		EndIf
		offset += 1
	EndWhile
EndFunction

Function SelectRepairItem()
	If RepairItems == None || RepairItems.Length == 0
		Return
	EndIf
	RepairItem selectedItem = RepairItems[Utility.RandomInt(0, RepairItems.Length - 1)]
	If selectedItem != None && selectedItem.StageToSet > 0 && !IsStageDone(selectedItem.StageToSet)
		SetStage(selectedItem.StageToSet)
	EndIf
EndFunction

Function PlaceRobot()
	MoveActorToMarker(Robot, ChosenRobotLocation)
EndFunction

Function ClearRepairSelection()
	ChosenRobotLocation.Clear()
	ChosenArea = None
	ClearLocalSelection()
EndFunction

Event OnQuestShutdown()
	ClearRepairSelection()
	Parent.OnQuestShutdown()
EndEvent
