Event OnLoad()
    If UnlockIfQuestCompleted != None && UnlockIfQuestCompleted.IsCompleted()
        Lock(False)
    EndIf
EndEvent
