Event OnLoad()
    If TargetQuest != None && TargetQuest.IsCompleted()
        Lock(False)
    EndIf
EndEvent
