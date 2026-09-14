Function Fragment_Stage_0010_Item_00()
    Actor vertibirdRef = Alias_Vertibird.GetReference() as Actor
    If vertibirdRef == None
        Return
    EndIf
    vertibirdRef.Enable()
    RegisterForRemoteEvent(vertibirdRef, "OnDeath")
    vertibirdRef.EvaluatePackage()
    SQ_WorkshopVertibirdSuccessMessage.Show()
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor vertibirdRef = Alias_Vertibird.GetReference() as Actor
    If vertibirdRef != None
        UnregisterForRemoteEvent(vertibirdRef, "OnDeath")
    EndIf
    If SQ_WorkshopVertibirdReturnMessage != None
        SQ_WorkshopVertibirdReturnMessage.Show()
    EndIf
    Stop()
    If vertibirdRef != None
        vertibirdRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor vertibirdRef = Alias_Vertibird.GetReference() as Actor
    If vertibirdRef != None
        UnregisterForRemoteEvent(vertibirdRef, "OnDeath")
    EndIf
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    Actor vertibirdRef = Alias_Vertibird.GetReference() as Actor
    If akSender != vertibirdRef
        Return
    EndIf
    SQ_WorkshopVertibirdDeathMessage.Show()
    UnregisterForRemoteEvent(akSender, "OnDeath")
    Stop()
EndEvent
