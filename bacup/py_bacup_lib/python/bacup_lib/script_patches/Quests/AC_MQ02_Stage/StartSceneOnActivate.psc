Event OnAliasInit()
    OwningQuest = GetOwningQuest()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If ActivatedByAlias != None && akActionRef != ActivatedByAlias.GetReference()
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If OwningQuest == None || SceneToStart == None
        Return
    EndIf
    If Stage_Prereq >= 0 && !OwningQuest.IsStageDone(Stage_Prereq)
        Return
    EndIf
    If Stage_TurnOff >= 0 && OwningQuest.IsStageDone(Stage_TurnOff)
        Return
    EndIf
    If SoundToPlay != None
        SoundToPlay.Play(GetReference())
    EndIf
    If SecondsToWait > 0.0
        Utility.Wait(SecondsToWait)
    EndIf
    If !SceneToStart.IsPlaying()
        SceneToStart.Start()
    EndIf
EndEvent
