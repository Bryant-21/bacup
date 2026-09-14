Function Fragment_End(Actor akActor)
    Actor showman = IntroShowman.GetActorReference()
    If showman != None
        showman.Disable()
    EndIf
EndFunction
