; FO76 stripped this greeting's end fragment. Every bounty-giver greeting gated by the
; owning quest points the player to the Last Resort, so it runs the quest's checkpoints in
; order: met (150), Highway Town map marker (200), pointer quest (500).
Function Fragment_End(ObjectReference akSpeakerRef)
    Quest bountyGiver = GetOwningQuest()
    If bountyGiver == None
        Return
    EndIf
    bountyGiver.SetStage(150)
    bountyGiver.SetStage(200)
    bountyGiver.SetStage(500)
EndFunction
