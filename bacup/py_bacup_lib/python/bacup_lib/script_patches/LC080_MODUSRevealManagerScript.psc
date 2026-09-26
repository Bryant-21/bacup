Event OnCellLoad()
    CheckMODUSRevealStateRMI(Game.GetPlayer())
EndEvent

Function CheckMODUSRevealStateRMI(Actor source)
    If source == None || source != Game.GetPlayer()
        Return
    EndIf
    SetMODUSRevealStateClient(IsLocalRevealCompleted(source))
EndFunction

Bool Function IsLocalRevealCompleted(Actor source)
    Bool completed = source != None && MODUSRevealCompletedValue != None && source.GetValue(MODUSRevealCompletedValue) > 0.0
    If MODUSRevealCompletedQuest != None
        completed = completed || MODUSRevealCompletedQuest.IsStageDone(80) || MODUSRevealCompletedQuest.IsCompleted()
    EndIf
    Return completed
EndFunction

Function StartMODUSRevealClient()
    Actor player = Game.GetPlayer()
    If player != None && MODUSRevealCompletedValue != None
        player.SetValue(MODUSRevealCompletedValue, 1.0)
    EndIf
    SendCustomEvent("MODUSRevealStart")
EndFunction
