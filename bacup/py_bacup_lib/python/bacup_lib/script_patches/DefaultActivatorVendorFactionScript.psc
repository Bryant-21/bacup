Event OnLoad()
    Actor selfActor = (Self as ObjectReference) as Actor
    If selfActor != None && TalkingActivatorVendorFaction != None
        selfActor.AddToFaction(TalkingActivatorVendorFaction)
    EndIf
EndEvent
