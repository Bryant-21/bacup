Scriptname B21:WorkshopCollector Extends ObjectReference

LeveledItem Property Produce Auto Const
GlobalVariable Property IntervalHours Auto Const
Int Property MaxStoredItems = 50 Auto Const
; FO76 scales output per interval by the resource row value (a large water purifier is 5).
Int Property ProduceCount = 1 Auto Const
Bool Property RequiresPower = false Auto Const

; Terminal-driven collectors (ArmCo ammo appliance, mode Collectrons): the terminal sets one
; of these actor values to 1 on this reference. No entry reading 1 falls back to Produce.
ActorValue[] Property ModeActorValues Auto Const
LeveledItem[] Property ModeProduce Auto Const
GlobalVariable[] Property ModeIntervalHours Auto Const
Int[] Property ModeMaxStoredItems Auto Const

Int ProductionTimerID = 1
Int NoMode = -1
Int ModeUnseen = -2

Float fLastProduced = -1.0
; Power is only observable while the reference is loaded, so time spent unloaded
; is credited according to the state it was last seen in.
Bool bPowered = false
Int iLastMode = -2

Event OnInit()
    fLastProduced = Utility.GetCurrentGameTime()
    StartProductionTimer()
EndEvent

Event OnLoad()
    Accrue()
    bPowered = IsPowered()
    StartProductionTimer()
EndEvent

Event OnPowerOn(ObjectReference akPowerGenerator)
    Accrue()
    bPowered = true
EndEvent

Event OnPowerOff()
    Accrue()
    bPowered = false
EndEvent

Event OnUnload()
    CancelTimerGameTime(ProductionTimerID)
EndEvent

Event OnTimerGameTime(int aiTimerID)
    If aiTimerID != ProductionTimerID
        Return
    EndIf
    Accrue()
    If Is3DLoaded()
        bPowered = IsPowered()
    EndIf
    StartProductionTimer()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Accrue()
    bPowered = IsPowered()
EndEvent

Function StartProductionTimer()
    GlobalVariable interval = ActiveInterval(ActiveMode())
    If interval == None
        Return
    EndIf
    Float hours = interval.GetValue()
    If hours > 0.0
        StartTimerGameTime(hours, ProductionTimerID)
    EndIf
EndFunction

; Index of the selected mode, NoMode when this is a single-output collector or none is selected.
Int Function ActiveMode()
    If ModeActorValues == None
        Return NoMode
    EndIf
    Int index = 0
    While index < ModeActorValues.Length
        If ModeActorValues[index] != None && GetValue(ModeActorValues[index]) == 1.0
            Return index
        EndIf
        index += 1
    EndWhile
    Return NoMode
EndFunction

LeveledItem Function ActiveProduce(Int aiMode)
    If aiMode >= 0 && ModeProduce != None && aiMode < ModeProduce.Length && ModeProduce[aiMode] != None
        Return ModeProduce[aiMode]
    EndIf
    Return Produce
EndFunction

GlobalVariable Function ActiveInterval(Int aiMode)
    If aiMode >= 0 && ModeIntervalHours != None && aiMode < ModeIntervalHours.Length && ModeIntervalHours[aiMode] != None
        Return ModeIntervalHours[aiMode]
    EndIf
    Return IntervalHours
EndFunction

Int Function ActiveCap(Int aiMode)
    If aiMode >= 0 && ModeMaxStoredItems != None && aiMode < ModeMaxStoredItems.Length && ModeMaxStoredItems[aiMode] > 0
        Return ModeMaxStoredItems[aiMode]
    EndIf
    Return MaxStoredItems
EndFunction

Function Accrue()
    Float now = Utility.GetCurrentGameTime()
    Int mode = ActiveMode()
    If iLastMode == ModeUnseen
        iLastMode = mode
    ElseIf mode != iLastMode
        ; Selecting another output starts a fresh batch instead of repricing the old interval.
        iLastMode = mode
        fLastProduced = now
        Return
    EndIf

    LeveledItem output = ActiveProduce(mode)
    GlobalVariable interval = ActiveInterval(mode)
    Int cap = ActiveCap(mode)
    If output == None || interval == None || cap <= 0
        Return
    EndIf

    Float intervalDays = interval.GetValue() / 24.0
    If intervalDays <= 0.0
        Return
    EndIf

    ; Unseeded (script newly attached to an existing reference) or a clock that
    ; moved backwards would otherwise bank an unbounded backlog in one step.
    If fLastProduced < 0.0 || fLastProduced > now
        fLastProduced = now
        Return
    EndIf

    Int elapsedIntervals = ((now - fLastProduced) / intervalDays) as Int
    If elapsedIntervals <= 0
        Return
    EndIf
    ; Consume every whole interval even when the container is full, so time spent
    ; at capacity does not accrue a burst that lands the moment it is emptied.
    fLastProduced += (elapsedIntervals as Float) * intervalDays
    If RequiresPower && !bPowered
        Return
    EndIf
    ; A full container cannot take more than its cap, and a list that sometimes rolls
    ; nothing must not spin through a months-long backlog.
    If elapsedIntervals > cap
        elapsedIntervals = cap
    EndIf

    ; Capacity comes from the live contents: FO4 only delivers inventory events
    ; through an inventory filter, so a removal-tracked counter never drains.
    While elapsedIntervals > 0 && GetItemCount(None) < cap
        Int roll = 0
        While roll < ProduceCount && GetItemCount(None) < cap
            AddItem(output, 1, true)
            roll += 1
        EndWhile
        elapsedIntervals -= 1
    EndWhile
EndFunction
