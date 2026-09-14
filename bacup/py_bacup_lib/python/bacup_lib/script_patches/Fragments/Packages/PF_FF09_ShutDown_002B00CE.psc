Function Fragment_Begin(Actor akActor)
    Actor botRef = bot.GetActorReference()
    If botRef != None
        botRef.Disable()
    EndIf
EndFunction
