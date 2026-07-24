Event OnLoad()
    ; TODO
    Actor selfActor = Self as Actor
    If selfActor != None && TalkingActivatorVendorFaction != None
        selfActor.AddToFaction(TalkingActivatorVendorFaction)
    EndIf
EndEvent
