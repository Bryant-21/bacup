Function Fragment_Phase_02_End()
    W05_MQA_206P_QuestScript owningQuest = GetOwningQuest() as W05_MQA_206P_QuestScript
    If owningQuest && owningQuest.JohnnyRobberyFurniture && owningQuest.Alias_JohnnyGoldMarker
        ObjectReference robberyFurniture = owningQuest.JohnnyRobberyFurniture.GetReference()
        ObjectReference robberyMarker = owningQuest.Alias_JohnnyGoldMarker.GetReference()
        If robberyFurniture && robberyMarker
            robberyFurniture.MoveTo(robberyMarker)
        EndIf
    EndIf
EndFunction
