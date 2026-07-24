Event OnLocationChange(ObjectReference akSenderRef, Location akOldLoc, Location akNewLoc)
    PopulateKeyCollection()
EndEvent

Event OnPlayerLoadGame(ObjectReference akSenderRef)
    PopulateKeyCollection()
EndEvent

Function PopulateKeyCollection()
    If GetOwningQuest() != None && GetOwningQuest().GetStage() >= PreReqStage && KeyCollection != None && KeyCollection.GetCount() == 0 && AccessCardSpawn != None && AccessCardSpawn.GetReference() != None
        int i = 0
        While i < TargetKeys.Length
            ObjectReference spawnedKey = AccessCardSpawn.GetReference().PlaceAtMe(TargetKeys[i])
            If spawnedKey != None
                KeyCollection.AddRef(spawnedKey)
            EndIf
            i += 1
        EndWhile
        If CompleteStage > 0
            GetOwningQuest().SetStage(CompleteStage)
        EndIf
    EndIf
EndFunction
