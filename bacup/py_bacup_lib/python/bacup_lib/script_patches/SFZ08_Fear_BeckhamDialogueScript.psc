; Beckham's trade topic is gated on SFZ08_Fear_VendorKeyword, which players carry while in EligiblePlayers.
; Event completion (stage 100, set by Irrational Fear's thank-you stage) opens his vendor to the player; FO76's
; server-side vendor timer is stood in for by Beckham unloading, which sets 1000 and closes it again.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        EligiblePlayers.AddRef(Game.GetPlayer())
        ObjectReference beckhamRef = Beckham.GetReference()
        If beckhamRef != None
            RegisterForRemoteEvent(beckhamRef, "OnUnload")
        EndIf
    ElseIf auiStageID == 1000
        EligiblePlayers.RemoveAll()
    EndIf
EndEvent

Event ObjectReference.OnUnload(ObjectReference akSender)
    UnregisterForRemoteEvent(akSender, "OnUnload")
    SetStage(1000)
EndEvent
