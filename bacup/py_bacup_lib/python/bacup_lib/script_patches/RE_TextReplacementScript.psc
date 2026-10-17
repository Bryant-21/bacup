; Random-encounter notes whose text names a dungeon (<Alias=DUNGEON>). The player taking Alias_Note sets
; NotePickedUpStage. StoryManagerKeyword started FO76's server group-quest instance and has no FO4 listener.
Event OnQuestInit()
    ObjectReference note = Alias_Note.GetReference()
    If note != None
        RegisterForRemoteEvent(note, "OnContainerChanged")
    EndIf
EndEvent

Event ObjectReference.OnContainerChanged(ObjectReference akSender, ObjectReference akNewContainer, ObjectReference akOldContainer)
    If akNewContainer == Game.GetPlayer() && akSender == Alias_Note.GetReference()
        UnregisterForRemoteEvent(akSender, "OnContainerChanged")
        NotePickedUp()
    EndIf
EndEvent

Function NotePickedUp()
    If NotePickedUpStage > 0 && !IsStageDone(NotePickedUpStage)
        SetStage(NotePickedUpStage)
    EndIf
EndFunction
