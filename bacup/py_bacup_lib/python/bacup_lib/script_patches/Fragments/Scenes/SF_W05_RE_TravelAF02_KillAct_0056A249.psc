Function Fragment_Action_01()
    Actor scavengerRef = Scavenger.GetActorReference()
    If scavengerRef != None && Health != None
        scavengerRef.SetValue(Health, 0.0)
    EndIf
EndFunction
