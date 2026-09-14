ScriptName QF_MGRJzargoSpell01_000958B3 Extends Quest Hidden

ReferenceAlias Property Alias_Jzargo Auto Const
Form Property Scroll01 Auto Const
MusicType Property CompletionMusic Auto Const

Function Fragment_0()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    Game.GetPlayer().AddItem(Scroll01, 10)
EndFunction

Function Fragment_2()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_4()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_6()
    CompleteAllObjectives()
    Alias_Jzargo.GetActorReference().SetRelationshipRank(Game.GetPlayer(), 1)
    CompletionMusic.Add()
    Stop()
EndFunction

Function Fragment_8()
    FailAllObjectives()
    Stop()
EndFunction
